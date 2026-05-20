use serde_json::{Value, json};
use std::path::Path;
use std::process::{Command, Stdio};

fn bridge_bin() -> &'static str {
    env!("CARGO_BIN_EXE_market_data_bridge")
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
