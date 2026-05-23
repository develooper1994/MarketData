use market_data::{
    DataHub, InMemoryStorage, ManifestProvenanceTracker, RawSourceAdapter, SourceAdapterRegistry,
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
        _requested_asset_class: Option<&str>,
        _force_asset_class: bool,
    ) -> Result<HashMap<String, serde_json::Value>, market_data::providers::errors::ProviderError>
    {
        let mut out = HashMap::new();
        for ds in datasets {
            if ds == "tick" || ds == "kline" {
                out.insert(
                    ds.clone(),
                    json!([{ "timestamp_ms": 1716200000000_i64, "last": "10.5", "source": "mock" }]),
                );
            }
        }
        Ok(out)
    }

    fn discover_assets(&self, _limit: usize) -> Vec<String> {
        vec!["BTCUSDT".to_string()]
    }
}

#[test]
fn force_asset_class_blocks_unsupported_source() {
    let mut registry = SourceAdapterRegistry::default();
    registry.register("btcturk", Arc::new(MockAdapter));

    let mut hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        registry,
        market_data::streaming::StreamingAdapterRegistry::default(),
    );

    let result = hub
        .ingest(
            "btcturk",
            "BTCUSDT",
            vec!["tick".to_string()],
            "1m",
            1,
            false,
            Some("equity"),
            true,
        )
        .expect("ingest should return without crashing");

    assert_eq!(result.records.len(), 0);
    assert_eq!(result.dataset_coverage.get("tick"), Some(&0usize));
    assert!(result.source_issues.iter().any(|issue| {
        issue
            .get("reason")
            .map(|r| r.contains("unsupported_asset_class:equity"))
            .unwrap_or(false)
    }));
}

#[test]
fn force_asset_class_allows_supported_source() {
    let mut registry = SourceAdapterRegistry::default();
    registry.register("btcturk", Arc::new(MockAdapter));

    let mut hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        registry,
        market_data::streaming::StreamingAdapterRegistry::default(),
    );

    let result = hub
        .ingest(
            "btcturk",
            "BTCUSDT",
            vec!["tick".to_string()],
            "1m",
            1,
            false,
            Some("crypto_spot"),
            true,
        )
        .expect("ingest should return without crashing");

    assert!(result.records.len() > 0);
    assert_eq!(result.dataset_coverage.get("tick"), Some(&1usize));
}
