use httpmock::Method::GET;
use httpmock::MockServer;
use market_data::{DataHub, Etl, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry};
use serde_json::{Value, json};
use std::sync::Arc;

struct TestKapAdapter {
    base: String,
    client: reqwest::blocking::Client,
}

impl TestKapAdapter {
    fn new(base: String) -> Self {
        Self { base, client: reqwest::blocking::Client::new() }
    }
}

impl market_data::hub::RawSourceAdapter for TestKapAdapter {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
    ) -> Result<std::collections::HashMap<String, Value>, market_data::providers::errors::ProviderError> {
        let mut out = std::collections::HashMap::new();
        for ds in datasets {
            if ds == "news" || ds == "corporate_actions" || ds == "fundamentals" {
                let url = format!("{}/tr/api/disclosures?company={}", self.base, symbol);
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
fn kap_news_fetch_via_mock() {
    let server = MockServer::start();
    let body = std::fs::read_to_string("tests/fixtures/kap_disclosures.json").expect("read fixture");
    let _m = server.mock(|when, then| {
        when.method(GET).path("/tr/api/disclosures").query_param("company", "BTCUSDT");
        then.status(200).header("Content-Type", "application/json").body(body);
    });

    let mut registry = SourceAdapterRegistry::default();
    registry.register("kap", Arc::new(TestKapAdapter::new(server.base_url())));

    let hub = DataHub::with_components(Box::new(InMemoryStorage::default()), ManifestProvenanceTracker::new(None::<&str>), registry, market_data::streaming::StreamingAdapterRegistry::default());
    let etl = Etl::new(hub)
        .source("kap")
        .select_assets(vec!["BTCUSDT".to_string()])
        .fetch(vec!["news".to_string()])
        .expect("etl fetch should succeed");

    assert_eq!(etl.results().len(), 1);
    let result = &etl.results()[0];
    assert!(!result.records.is_empty());
    assert_eq!(result.records[0].source, "kap");
}
