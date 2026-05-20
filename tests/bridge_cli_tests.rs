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
    assert_eq!(payload["supported_datasets"], json!(["kline"]));
    assert_eq!(payload["bridge_contract"]["raw_datasets"], true);
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
