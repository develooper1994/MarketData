use market_data::{
    DataHub, Etl, InMemoryStorage, ManifestProvenanceTracker, RawSourceAdapter,
    SourceAdapterRegistry,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

struct MockAdapter;

impl RawSourceAdapter for MockAdapter {
    fn fetch_raw(
        &self,
        _symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
    ) -> Result<HashMap<String, serde_json::Value>, market_data::providers::errors::ProviderError> {
        let mut out = HashMap::new();
        for dataset in datasets {
            if dataset == "kline" {
                out.insert(
                    dataset.clone(),
                    json!([[1716200000000_i64, "10", "11", "9", "10.5", "42"]]),
                );
            }
        }
        Ok(out)
    }

    fn discover_assets(&self, _limit: usize) -> Vec<String> {
        vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]
    }
}

#[test]
fn ingest_from_raw_builds_records_and_provenance() {
    let mut hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        SourceAdapterRegistry::default(),
        market_data::streaming::StreamingAdapterRegistry::default(),
    );

    let result = hub
        .ingest_from_raw(
            "offline",
            "BTCUSDT",
            vec!["kline".to_string()],
            HashMap::from([(
                "kline".to_string(),
                json!([[1716200000000_i64, "10", "11", "9", "10.5", "42"]]),
            )]),
            true,
        )
        .expect("ingestion should succeed");

    assert_eq!(result.records.len(), 1);
    assert_eq!(result.dataset_coverage.get("kline"), Some(&1));
    assert!(result.raw_datasets.contains_key("kline"));
    assert!(result.provenance.is_some());
    assert!(result.quality_report.passed, "quality report should pass");
    assert_eq!(result.records[0].domain, "market");
    assert_eq!(
        result.records[0].key,
        "offline:kline:BTCUSDT:1716200000000:1"
    );
}

#[test]
fn quality_detects_non_monotonic_kline_timestamps() {
    let mut hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        SourceAdapterRegistry::default(),
        market_data::streaming::StreamingAdapterRegistry::default(),
    );

    let result = hub
        .ingest_from_raw(
            "offline",
            "BTCUSDT",
            vec!["kline".to_string()],
            HashMap::from([(
                "kline".to_string(),
                json!([
                    [1716200000002_i64, "10", "11", "9", "10.5", "42"],
                    [1716200000001_i64, "10", "11", "9", "10.5", "42"]
                ]),
            )]),
            false,
        )
        .expect("ingestion should succeed");

    assert!(!result.quality_report.passed);
    assert!(
        result
            .quality_report
            .issues
            .iter()
            .any(|issue| issue.contains("Non-monotonic timestamps"))
    );
    assert_eq!(
        result.raw_datasets.get("kline"),
        Some(&json!([
            [1716200000002_i64, "10", "11", "9", "10.5", "42"],
            [1716200000001_i64, "10", "11", "9", "10.5", "42"]
        ]))
    );
}

#[test]
fn etl_fetches_via_registered_adapter() {
    let mut registry = SourceAdapterRegistry::default();
    registry.register("mock", Arc::new(MockAdapter));

    let hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        registry,
        market_data::streaming::StreamingAdapterRegistry::default(),
    );

    let etl = Etl::new(hub)
        .source("mock")
        .select_assets(vec!["BTCUSDT".to_string()])
        .fetch(vec!["kline".to_string()])
        .expect("etl fetch should succeed");

    assert_eq!(etl.results().len(), 1);
    assert_eq!(etl.results()[0].records.len(), 1);
    assert_eq!(
        etl.results()[0].records[0].key,
        "mock:kline:BTCUSDT:1716200000000:1"
    );
}

#[test]
fn default_registry_offline_adapter_supports_fetch_and_discovery() {
    let mut hub = DataHub::default();
    let result = hub
        .ingest(
            "offline",
            "BTCUSDT",
            vec!["kline".to_string()],
            "1m",
            10,
            false,
        )
        .expect("offline adapter ingest should succeed");
    assert_eq!(result.dataset_coverage.get("kline"), Some(&1));

    let assets = hub
        .discover_assets("offline", 2)
        .expect("offline discovery should succeed");
    assert_eq!(assets, vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]);
}

#[test]
fn ingest_from_raw_resolves_ohlcv_alias_to_kline() {
    let mut hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        SourceAdapterRegistry::default(),
        market_data::streaming::StreamingAdapterRegistry::default(),
    );

    let result = hub
        .ingest_from_raw(
            "offline",
            "BTCUSDT",
            vec!["ohlcv".to_string()],
            HashMap::from([(
                "ohlcv".to_string(),
                json!([[1716200000000_i64, "10", "11", "9", "10.5", "42"]]),
            )]),
            false,
        )
        .expect("ingestion should succeed");

    assert_eq!(result.requested_datasets, vec!["kline".to_string()]);
    assert_eq!(result.dataset_coverage.get("kline"), Some(&1));
    assert_eq!(result.records[0].domain, "market");
}
