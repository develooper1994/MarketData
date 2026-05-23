use httpmock::Method::GET;
use httpmock::MockServer;
use market_data::{
    DataHub, Etl, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry,
};
use serde_json::{Value, json};
use std::sync::Arc;

struct TestTefasAdapter {
    base: String,
    client: reqwest::blocking::Client,
}

impl TestTefasAdapter {
    fn new(base: String) -> Self {
        Self {
            base,
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl market_data::hub::RawSourceAdapter for TestTefasAdapter {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
        _requested_asset_class: Option<&str>,
        _force_asset_class: bool,
    ) -> Result<
        std::collections::HashMap<String, Value>,
        market_data::providers::errors::ProviderError,
    > {
        let mut out = std::collections::HashMap::new();
        for ds in datasets {
            if ds == "fundamentals" || ds == "corporate_actions" {
                let url = format!("{}/api/values?symbol={}", self.base, symbol);
                let resp = self.client.get(&url).send()?;
                let json_v = resp.json::<Value>()?;
                if json_v.is_array() {
                    out.insert(ds.clone(), json_v);
                } else {
                    out.insert(ds.clone(), json!([json_v]));
                }
            }
        }
        Ok(out)
    }
}

#[test]
fn tefas_fundamentals_fetch_via_mock() {
    let server = MockServer::start();

    let body = serde_json::json!([ {"date": "2024-01-01", "revenue": 1000 } ]);

    let _m = server.mock(|when, then| {
        when.method(GET)
            .path("/api/values")
            .query_param("symbol", "BTCUSDT");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(body.to_string());
    });

    let mut registry = SourceAdapterRegistry::default();
    registry.register("tefas", Arc::new(TestTefasAdapter::new(server.base_url())));

    let hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        registry,
        market_data::streaming::StreamingAdapterRegistry::default(),
    );
    let etl = Etl::new(hub)
        .source("tefas")
        .select_assets(vec!["BTCUSDT".to_string()])
        .fetch(vec!["fundamentals".to_string()])
        .expect("etl fetch should succeed");

    assert_eq!(etl.results().len(), 1);
    let result = &etl.results()[0];
    assert!(!result.records.is_empty());
    assert_eq!(result.records[0].source, "tefas");
}
