use serde_json::{Value, json};
use std::path::Path;
use std::process::{Command, Stdio};

fn bridge_bin() -> &'static str {
    env!("CARGO_BIN_EXE_market_data_bridge")
}

#[test]
fn help_command_prints_menu_and_examples() {
    let output = Command::new(bridge_bin())
        .arg("help")
        .output()
        .expect("help command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("USAGE"));
    assert!(stdout.contains("COMMANDS"));
    assert!(stdout.contains("COMMON FLOWS"));
    assert!(stdout.contains("ONLINE DATA (REAL FETCH)"));
    assert!(stdout.contains("MORE EXAMPLES"));
    assert!(stdout.contains("assert-contract"));
    assert!(stdout.contains("sources"));
    assert!(stdout.contains("capabilities"));
    assert!(stdout.contains("query-best-sources"));
    assert!(stdout.contains("recommend-sources"));
    assert!(stdout.contains("live-fetch"));
    assert!(stdout.contains("ingest"));
    assert!(stdout.contains("--json load_market_data"));
}

#[test]
fn help_flag_prints_menu() {
    let output = Command::new(bridge_bin())
        .arg("--help")
        .output()
        .expect("--help should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("market_data_bridge"));
    assert!(stdout.contains("ingest"));
}

#[test]
fn short_aliases_work_for_query_and_listing() {
    let list_output = Command::new(bridge_bin())
        .arg("ls")
        .output()
        .expect("ls alias should run");
    assert!(list_output.status.success());
    let list_payload: Value =
        serde_json::from_slice(&list_output.stdout).expect("ls output should be valid json");
    assert!(
        list_payload
            .as_array()
            .is_some_and(|arr| arr.iter().any(|v| v == "binance_futures"))
    );

    let query_output = Command::new(bridge_bin())
        .args(["qsf", "--dataset", "kline"])
        .output()
        .expect("qsf alias should run");
    assert!(query_output.status.success());
    let query_payload: Value =
        serde_json::from_slice(&query_output.stdout).expect("qsf output should be valid json");
    assert!(
        query_payload
            .as_array()
            .is_some_and(|arr| arr.iter().any(|v| v == "binance_futures"))
    );

    let matrix_output = Command::new(bridge_bin())
        .arg("qdm")
        .output()
        .expect("qdm alias should run");
    assert!(matrix_output.status.success());
    let matrix_payload: Value =
        serde_json::from_slice(&matrix_output.stdout).expect("qdm output should be valid json");
    assert_eq!(matrix_payload["kline"]["dataset"], "kline");
}

#[test]
fn no_args_prints_help_menu() {
    let output = Command::new(bridge_bin())
        .output()
        .expect("invocation without args should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("USAGE"));
    assert!(stdout.contains("help"));
}

#[test]
fn no_args_accepts_stdin_json_doctor_request() {
    let mut child = Command::new(bridge_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("invocation without args should run");

    serde_json::to_writer(
        child.stdin.take().expect("stdin should be available"),
        &json!({ "command": "doctor" }),
    )
    .expect("request should serialize");
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("doctor output should be valid json");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["binary"], "market_data_bridge");
    assert_eq!(payload["transport"], "stdin_json");
}

#[test]
fn doctor_reports_bridge_contract() {
    let output = Command::new(bridge_bin())
        .arg("doctor")
        .output()
        .expect("doctor command should run");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("doctor output should be valid json");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["binary"], "market_data_bridge");
    assert!(
        payload["supported_datasets"]
            .as_array()
            .is_some_and(|arr| arr.iter().any(|v| v == "kline"))
    );
    assert!(
        payload["supported_datasets"]
            .as_array()
            .is_some_and(|arr| arr.iter().any(|v| v == "tick"))
    );
    assert!(
        payload["bridge_contract"]["raw_datasets"]
            .as_bool()
            .unwrap_or(false)
    );
}

#[test]
fn ingest_round_trips_raw_payload_and_artifacts() {
    let records_dir = tempfile::tempdir().expect("records tempdir");
    let manifests_dir = tempfile::tempdir().expect("manifests tempdir");
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--datasets",
            "kline",
            "--asset-type",
            "crypto_spot",
            "--record-root",
            records_dir.path().to_str().expect("utf-8 records path"),
            "--manifest-root",
            manifests_dir.path().to_str().expect("utf-8 manifest path"),
            "--store",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    let input = json!({
        "kline": [[1716200000000_i64, "10", "11", "9", "10.5", "42"]],
    });
    serde_json::to_writer(
        child.stdin.take().expect("stdin should be available"),
        &input,
    )
    .expect("input should serialize");
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("bridge output should be valid json");

    assert_eq!(payload["dataset_coverage"]["kline"], 1);
    assert_eq!(payload["raw_datasets"], input);
    assert_eq!(payload["records"][0]["domain"], "market");
    assert_eq!(payload["records"][0]["asset_type"], "crypto_spot");
    assert_eq!(
        payload["records"][0]["key"],
        "offline:kline:BTCUSDT:1716200000000:1"
    );
    assert_eq!(payload["quality_report"]["passed"], true);
    assert!(
        payload["storage_receipts"][0]["location"]
            .as_str()
            .is_some_and(|path| Path::new(path).exists())
    );
    assert_eq!(payload["provenance"]["source_plugin_id"], "offline");
    assert!(manifests_dir.path().join("manifest-1.json").exists());
}

#[test]
fn ingest_without_stdin_payload_uses_offline_adapter() {
    let output = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--datasets",
            "kline",
            "--asset-type",
            "crypto_spot",
        ])
        .output()
        .expect("bridge ingest command should run");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("bridge output should be valid json");
    assert_eq!(payload["dataset_coverage"]["kline"], 1);
    assert_eq!(payload["records"][0]["domain"], "market");
    assert_eq!(payload["records"][0]["asset_type"], "crypto_spot");
}

#[test]
fn ingest_surfaces_missing_dataset_issues() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--datasets",
            "kline,trade",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    serde_json::to_writer(
        child.stdin.take().expect("stdin should be available"),
        &json!({
            "kline": [[1716200000000_i64, "10", "11", "9", "10.5", "42"]],
        }),
    )
    .expect("input should serialize");
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("bridge output should be valid json");

    assert_eq!(payload["dataset_coverage"]["kline"], 1);
    assert_eq!(payload["dataset_coverage"]["trade"], 0);
    assert!(
        payload["source_issues"]
            .as_array()
            .expect("source issues should be an array")
            .iter()
            .any(|issue| issue["reason"] == "missing_dataset:trade")
    );
}

#[test]
fn ingest_accepts_repeated_dataset_flag() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ing",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--dataset",
            "kline",
            "--dataset",
            "trade",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest alias command should run");

    serde_json::to_writer(
        child.stdin.take().expect("stdin should be available"),
        &json!({
            "kline": [[1716200000000_i64, "10", "11", "9", "10.5", "42"]],
            "trade": [{"t": 1716200000001_i64, "price": "10.5", "qty": "0.5"}]
        }),
    )
    .expect("input should serialize");
    let output = child.wait_with_output().expect("bridge should complete");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(payload["dataset_coverage"]["kline"], 1);
    assert_eq!(payload["dataset_coverage"]["trade"], 1);
}

#[test]
fn live_fetch_single_command_returns_rows_without_stdin_json() {
    let output = Command::new(bridge_bin())
        .args([
            "live-fetch",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--dataset",
            "tick",
            "--limit",
            "5",
        ])
        .output()
        .expect("live-fetch command should run");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    assert_eq!(payload["source"], "offline");
    assert_eq!(payload["symbol"], "BTCUSDT");
    assert_eq!(payload["dataset"], "tick");
    assert!(
        payload["rows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
}

#[test]
fn live_fetch_alias_lf_works() {
    let output = Command::new(bridge_bin())
        .args([
            "lf",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--dataset",
            "kline",
        ])
        .output()
        .expect("lf alias should run");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    assert_eq!(payload["dataset"], "kline");
    assert!(
        payload["rows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    assert!(payload.get("rows_by_dataset").is_none());
}

#[test]
fn live_fetch_multi_dataset_includes_rows_by_dataset() {
    let output = Command::new(bridge_bin())
        .args([
            "live-fetch",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--datasets",
            "kline,tick",
        ])
        .output()
        .expect("live-fetch command should run");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    assert!(
        payload["rows_by_dataset"]["kline"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    assert!(
        payload["rows_by_dataset"]["tick"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
}

#[test]
fn live_fetch_requires_source_flag() {
    let output = Command::new(bridge_bin())
        .args(["live-fetch", "--symbol", "BTCUSDT", "--dataset", "tick"])
        .output()
        .expect("live-fetch command should run");

    // Source is now optional: the bridge will attempt auto-selection/fallback.
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    assert!(payload.get("source").is_some());
}

#[test]
fn no_args_stdin_json_ingest_request_supports_structured_options() {
    let mut child = Command::new(bridge_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("invocation without args should run");

    serde_json::to_writer(
        child.stdin.take().expect("stdin should be available"),
        &json!({
            "command": "ingest",
            "source": "offline",
            "symbol": "BTCUSDT",
            "datasets": ["kline"],
            "asset_type": "crypto_spot"
        }),
    )
    .expect("request should serialize");
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("bridge output should be valid json");
    assert_eq!(payload["dataset_coverage"]["kline"], 1);
    assert_eq!(payload["records"][0]["asset_type"], "crypto_spot");
}

#[test]
fn capabilities_command_returns_all_sources() {
    let output = Command::new(bridge_bin())
        .arg("capabilities")
        .output()
        .expect("capabilities command should run");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("capabilities output should be valid json");
    let arr = payload
        .as_array()
        .expect("capabilities should be a json array");
    assert!(
        arr.len() >= 24,
        "expected at least 24 sources, got {}",
        arr.len()
    );
    let sources: Vec<&str> = arr.iter().filter_map(|c| c["source"].as_str()).collect();
    assert!(sources.contains(&"binance_futures"));
    assert!(sources.contains(&"binance_spot"));
    assert!(sources.contains(&"tefas_public"));
    assert!(sources.contains(&"offline_fallback"));
    // Each entry must have required fields
    for cap in arr {
        assert!(cap["source"].is_string());
        assert!(cap["asset_classes"].is_array());
        assert!(cap["datasets"].is_array());
    }
}

#[test]
fn sources_command_returns_sorted_name_list() {
    let output = Command::new(bridge_bin())
        .arg("sources")
        .output()
        .expect("sources command should run");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("sources output should be valid json");
    let names = payload
        .as_array()
        .expect("sources should be a json array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"binance_futures"));
    assert!(names.contains(&"binance_spot"));
    assert!(names.contains(&"yahoo_unofficial"));
}

#[test]
fn query_sources_for_returns_filtered_list() {
    let output = Command::new(bridge_bin())
        .args(["query-sources-for", "--dataset", "kline"])
        .output()
        .expect("query-sources-for command should run");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout)
        .expect("query-sources-for output should be valid json");
    let names = payload
        .as_array()
        .expect("query-sources-for should return a json array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"binance_futures"));
    assert!(names.contains(&"stooq"));
}

#[test]
fn query_sources_for_require_live_excludes_non_realtime() {
    let output = Command::new(bridge_bin())
        .args(["query-sources-for", "--dataset", "kline", "--require-live"])
        .output()
        .expect("query-sources-for --require-live command should run");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout)
        .expect("query-sources-for output should be valid json");
    let names = payload
        .as_array()
        .expect("query-sources-for should return a json array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"binance_futures"));
    assert!(!names.contains(&"stooq"), "stooq does not support realtime");
}

#[test]
fn query_best_sources_returns_ranked_rows() {
    let output = Command::new(bridge_bin())
        .args([
            "query-best-sources",
            "--dataset",
            "kline",
            "--asset-class",
            "crypto_spot",
            "--limit",
            "3",
        ])
        .output()
        .expect("query-best-sources command should run");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("query-best-sources output should be valid");
    let rows = payload
        .as_array()
        .expect("query-best-sources should return json array");
    assert!(!rows.is_empty());
    assert!(rows[0]["source"].is_string());
}

#[test]
fn query_source_summary_returns_source_metadata() {
    let output = Command::new(bridge_bin())
        .args(["query-source-summary", "--source", "binance_futures"])
        .output()
        .expect("query-source-summary command should run");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout)
        .expect("query-source-summary output should be valid");
    assert_eq!(payload["source"], "binance_futures");
    assert!(payload["datasets"].is_array());
}

#[test]
fn query_dataset_summary_returns_counts() {
    let output = Command::new(bridge_bin())
        .args(["query-dataset-summary", "--dataset", "kline"])
        .output()
        .expect("query-dataset-summary command should run");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout)
        .expect("query-dataset-summary output should be valid");
    assert_eq!(payload["dataset"], "kline");
    assert!(payload["source_count"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn query_dataset_matrix_returns_machine_readable_coverage() {
    let output = Command::new(bridge_bin())
        .arg("query-dataset-matrix")
        .output()
        .expect("query-dataset-matrix command should run");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout)
        .expect("query-dataset-matrix output should be valid");
    assert_eq!(payload["kline"]["dataset"], "kline");
    assert!(payload["kline"]["source_count"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn recommend_sources_returns_use_case_results() {
    let output = Command::new(bridge_bin())
        .args([
            "recommend-sources",
            "--use-case",
            "crypto_backtest",
            "--limit",
            "2",
        ])
        .output()
        .expect("recommend-sources command should run");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("recommend-sources output should be valid");
    let rows = payload
        .as_array()
        .expect("recommend-sources should return json array");
    assert!(!rows.is_empty());
    assert!(rows[0]["source"].is_string());
}

#[test]
fn supported_use_cases_returns_known_cases() {
    let output = Command::new(bridge_bin())
        .arg("supported-use-cases")
        .output()
        .expect("supported-use-cases command should run");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("supported-use-cases output should be valid");
    let rows = payload
        .as_array()
        .expect("supported-use-cases should return json array");
    assert!(rows.iter().any(|v| v == "crypto_live_trading"));
    assert!(rows.iter().any(|v| v == "fundamental_screening"));
}

#[test]
fn ingest_normalizes_trade_dataset() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--datasets",
            "trade",
            "--asset-type",
            "crypto_spot",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    let input = json!({
        "trade": [{"t": 1716200000000_i64, "price": "100.0", "qty": "1.5", "side": "buy"}],
    });
    serde_json::to_writer(child.stdin.take().expect("stdin"), &input).unwrap();
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["dataset_coverage"]["trade"], 1);
    assert_eq!(payload["records"][0]["domain"], "market");
}

#[test]
fn ingest_normalizes_orderbook_dataset() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--datasets",
            "orderbook",
            "--asset-type",
            "crypto_spot",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    let input = json!({
        "orderbook": {"bids": [["10.0", "1"]], "asks": [["10.1", "1"]], "timestamp_ms": 1716200000000_i64},
    });
    serde_json::to_writer(child.stdin.take().expect("stdin"), &input).unwrap();
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["dataset_coverage"]["orderbook"], 1);
    assert_eq!(payload["records"][0]["domain"], "market");
}

#[test]
fn ingest_normalizes_funding_dataset() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--datasets",
            "funding",
            "--asset-type",
            "crypto_perpetual",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    let input = json!({
        "funding": [{"fundingTime": 1716200000000_i64, "fundingRate": "0.0001"}],
    });
    serde_json::to_writer(child.stdin.take().expect("stdin"), &input).unwrap();
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["dataset_coverage"]["funding"], 1);
    assert_eq!(payload["records"][0]["domain"], "market");
}

#[test]
fn ingest_normalizes_macro_dataset() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "fred",
            "--symbol",
            "FEDFUNDS",
            "--datasets",
            "macro",
            "--asset-type",
            "macro_indicator",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    let input = json!({
        "macro": [{"date": "2024-01-01T00:00:00Z", "value": "5.5", "series_id": "FEDFUNDS"}],
    });
    serde_json::to_writer(child.stdin.take().expect("stdin"), &input).unwrap();
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["dataset_coverage"]["macro"], 1);
    assert_eq!(payload["records"][0]["domain"], "macro");
}

#[test]
fn ingest_normalizes_news_dataset() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "gdelt",
            "--symbol",
            "AAPL",
            "--datasets",
            "news",
            "--asset-type",
            "equity",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    let input = json!({
        "news": [{"title": "Apple reports earnings", "url": "https://example.com", "publishedAt": "2024-01-01T00:00:00Z"}],
    });
    serde_json::to_writer(child.stdin.take().expect("stdin"), &input).unwrap();
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["dataset_coverage"]["news"], 1);
    assert_eq!(payload["records"][0]["domain"], "news");
}

#[test]
fn ingest_normalizes_fundamentals_dataset() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "financial_modeling_prep",
            "--symbol",
            "AAPL",
            "--datasets",
            "fundamentals",
            "--asset-type",
            "equity",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    let input = json!({
        "fundamentals": [{"date": "2024-01-01T00:00:00Z", "revenue": 100000000, "eps": "1.25"}],
    });
    serde_json::to_writer(child.stdin.take().expect("stdin"), &input).unwrap();
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["dataset_coverage"]["fundamentals"], 1);
    assert_eq!(payload["records"][0]["domain"], "fundamentals");
}

#[test]
fn ingest_normalizes_corporate_actions_dataset() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "financial_modeling_prep",
            "--symbol",
            "AAPL",
            "--datasets",
            "corporate_actions",
            "--asset-type",
            "equity",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    let input = json!({
        "corporate_actions": [{"date": "2024-01-01T00:00:00Z", "type": "dividend", "amount": "0.25"}],
    });
    serde_json::to_writer(child.stdin.take().expect("stdin"), &input).unwrap();
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["dataset_coverage"]["corporate_actions"], 1);
    assert_eq!(payload["records"][0]["domain"], "fundamentals");
}

#[test]
fn doctor_reports_contract_version() {
    let output = Command::new(bridge_bin())
        .arg("doctor")
        .output()
        .expect("doctor command should run");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        payload["contract_version"].as_str().is_some(),
        "doctor should report contract_version"
    );
    assert!(
        payload["bridge_contract"]["contract_version"]
            .as_str()
            .is_some(),
        "bridge_contract object should include contract_version"
    );
}

#[test]
fn assert_contract_succeeds_for_matching_version() {
    let output = Command::new(bridge_bin())
        .args(["assert-contract", "--expected", "1"])
        .output()
        .expect("assert-contract command should run");

    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["expected"], "1");
    assert_eq!(payload["actual"], "1");
    assert_eq!(payload["compatible"], true);
}

#[test]
fn assert_contract_fails_for_mismatched_version() {
    let output = Command::new(bridge_bin())
        .args(["assert-contract", "--expected", "999"])
        .output()
        .expect("assert-contract command should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("contract version mismatch"));
}

#[test]
fn ingest_normalizes_tick_dataset() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--datasets",
            "tick",
            "--asset-type",
            "crypto_spot",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    let input = json!({
        "tick": [{"timestamp_ms": 1716200000000_i64, "bid": "10.5", "ask": "10.6", "last": "10.55"}],
    });
    serde_json::to_writer(
        child.stdin.take().expect("stdin should be available"),
        &input,
    )
    .expect("input should serialize");
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("bridge output should be valid json");
    assert_eq!(payload["dataset_coverage"]["tick"], 1);
    assert_eq!(payload["records"][0]["domain"], "market");
}

#[test]
fn live_adapters_are_opt_in_and_do_not_crash() {
    if std::env::var("MARKET_DATA_LIVE_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping live adapter test (set MARKET_DATA_LIVE_TESTS=1 to enable)");
        return;
    }

    let cases = vec![
        (
            "binance_futures",
            "BTCUSDT",
            vec!["tick", "kline", "trade", "orderbook", "funding"],
        ),
        (
            "binance_spot",
            "BTCUSDT",
            vec!["tick", "kline", "trade", "orderbook"],
        ),
        (
            "bybit_linear",
            "BTCUSDT",
            vec!["tick", "kline", "trade", "orderbook", "funding"],
        ),
        (
            "coinbase_spot",
            "BTC-USD",
            vec!["tick", "kline", "trade", "orderbook"],
        ),
        ("stooq", "aapl.us", vec!["kline"]),
        ("yahoo_unofficial", "AAPL", vec!["tick", "kline"]),
        ("coingecko", "bitcoin", vec!["tick", "kline"]),
        (
            "kraken_spot",
            "XBTUSD",
            vec!["tick", "kline", "trade", "orderbook"],
        ),
        ("frankfurter_fx", "USD", vec!["macro"]),
        ("ecb", "USD", vec!["tick", "macro"]),
        ("world_bank", "NY.GDP.MKTP.CD", vec!["macro"]),
        ("gdelt", "bitcoin", vec!["news"]),
        ("hacker_news", "bitcoin", vec!["news"]),
    ];

    for (source, symbol, datasets) in cases {
        let request = json!({
            "source": source,
            "symbol": symbol,
            "datasets": datasets,
            "timeframe": "1m",
            "limit": 50,
            "allow_partial": true,
            "store": false,
            "fetch_options": {},
        });

        let mut child = Command::new(bridge_bin())
            .args(["ingest", "--json"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("json ingest should run");

        serde_json::to_writer(
            child.stdin.take().expect("stdin should be available"),
            &request,
        )
        .expect("request should serialize");

        let output = child.wait_with_output().expect("bridge should complete");
        assert!(
            output.status.success(),
            "json ingest failed for source={source}"
        );

        let payload: Value =
            serde_json::from_slice(&output.stdout).expect("bridge output should be valid json");
        let coverage_total: i64 = payload["dataset_coverage"]
            .as_object()
            .map(|rows| rows.values().map(|v| v.as_i64().unwrap_or(0)).sum::<i64>())
            .unwrap_or(0);
        let issues = payload["source_issues"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let has_allowed_issue = issues.iter().any(|issue| {
            issue["reason"].as_str().is_some_and(|reason| {
                reason.starts_with("rate_limited:")
                    || reason.starts_with("network_error:")
                    || reason.starts_with("api_key_required:")
                    || reason.starts_with("unsupported_dataset:")
            })
        });
        let has_allowed_quality_issue =
            payload["quality_report"]["issues"]
                .as_array()
                .is_some_and(|rows| {
                    rows.iter().any(|issue| {
                        issue
                            .as_str()
                            .is_some_and(|reason| reason.contains("No records fetched for request"))
                    })
                });

        assert!(
            coverage_total > 0 || has_allowed_issue || has_allowed_quality_issue,
            "expected coverage or allowed source issue for source={source}; payload={payload}"
        );
    }
}

#[test]
fn unknown_command_exits_nonzero_and_shows_help() {
    let output = Command::new(bridge_bin())
        .arg("notacommand")
        .output()
        .expect("invocation with unknown command should run");

    assert!(
        !output.status.success(),
        "unknown command should exit with non-zero status"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown command"),
        "stderr should mention 'unknown command'"
    );
    assert!(
        stderr.contains("market_data_bridge help"),
        "stderr should explicitly point users to the help command"
    );
    assert!(
        stderr.contains("USAGE") || stderr.contains("market_data_bridge"),
        "stderr should include help/usage guidance"
    );
}

#[test]
fn ingest_accepts_kline_alias_ohlcv() {
    let mut child = Command::new(bridge_bin())
        .args([
            "ingest",
            "--source",
            "offline",
            "--symbol",
            "BTCUSDT",
            "--dataset",
            "ohlcv",
            "--asset-type",
            "crypto_spot",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("bridge ingest command should run");

    let input = json!({
        "ohlcv": [[1716200000000_i64, "10", "11", "9", "10.5", "42"]],
    });
    serde_json::to_writer(
        child.stdin.take().expect("stdin should be available"),
        &input,
    )
    .expect("input should serialize");
    let output = child.wait_with_output().expect("bridge should complete");

    assert!(output.status.success());
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("bridge output should be valid json");
    assert_eq!(payload["dataset_coverage"]["kline"], 1);
    assert_eq!(payload["requested_datasets"][0], "kline");
}

#[test]
fn prelude_exports_are_usable() {
    use market_data::prelude::{
        DataHub, InMemoryStorage, IngestResult, ManifestProvenanceTracker, QualityReport,
        SourceAdapterRegistry, StorageReceipt,
    };
    use std::collections::HashMap;

    let mut hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        SourceAdapterRegistry::default(),
        market_data::streaming::StreamingAdapterRegistry::default(),
    );
    let result: IngestResult = hub
        .ingest_from_raw(
            "offline",
            "BTCUSDT",
            vec!["kline".to_string()],
            HashMap::from([(
                "kline".to_string(),
                serde_json::json!([[1716200000000_i64, "10", "11", "9", "10.5", "42"]]),
            )]),
            false,
            None,
            false,
        )
        .expect("prelude ingest should succeed");

    let report: &QualityReport = &result.quality_report;
    assert!(report.passed);
    assert!(report.issues.is_empty());

    let receipts: &Vec<StorageReceipt> = &result.storage_receipts;
    assert!(receipts.is_empty(), "no storage with store=false");

    assert_eq!(result.records.len(), 1);
    assert_eq!(result.records[0].domain, "market");
}
