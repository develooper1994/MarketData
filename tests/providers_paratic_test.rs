use httpmock::Method::GET;
use httpmock::MockServer;
use market_data::{DataHub, Etl, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry};
use serde_json::Value;
use std::sync::Arc;

struct TestParaticAdapter {
    base: String,
    client: reqwest::blocking::Client,
}

impl TestParaticAdapter {
    fn new(base: String) -> Self {
        Self { base, client: reqwest::blocking::Client::new() }
    }
}

impl market_data::hub::RawSourceAdapter for TestParaticAdapter {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
    ) -> Result<std::collections::HashMap<String, Value>, market_data::providers::errors::ProviderError> {
        let mut out = std::collections::HashMap::new();
        for ds in datasets {
            if ds == "tick" {
                let url = format!("{}/API/g.php?symbol={}", self.base, symbol);
                let resp = self.client.get(&url).send()?;
                let json_v = resp.json::<Value>()?;
                // Ensure we always return an array payload for downstream normalization
                if json_v.is_array() {
                    out.insert(ds.clone(), json_v);
                } else {
                    out.insert(ds.clone(), Value::Array(vec![json_v]));
                }
            }
        }
        Ok(out)
    }
}

#[test]
fn paratic_tick_fetch_via_mock() {
    let server = MockServer::start();

    let body = serde_json::json!({"last": 111.1});

    let _m = server.mock(|when, then| {
        when.method(GET).path("/API/g.php").query_param("symbol", "BTCUSDT");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(body.to_string());
    });

    let mut registry = SourceAdapterRegistry::default();
    registry.register("paratic", Arc::new(TestParaticAdapter::new(server.base_url())));

    let hub = DataHub::with_components(Box::new(InMemoryStorage::default()), ManifestProvenanceTracker::new(None::<&str>), registry);
    let etl = Etl::new(hub)
        .source("paratic")
        .select_assets(vec!["BTCUSDT".to_string()])
        .fetch(vec!["tick".to_string()])
        .expect("etl fetch should succeed");

    assert_eq!(etl.results().len(), 1);
    let result = &etl.results()[0];
    assert!(!result.records.is_empty());
    assert_eq!(result.records[0].source, "paratic");
}
