use httpmock::Method::GET;
use httpmock::MockServer;
use market_data::{DataHub, Etl, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry};
use serde_json::Value;
use std::sync::Arc;

struct TestBtcturkAdapter {
    base: String,
    client: reqwest::blocking::Client,
}

impl TestBtcturkAdapter {
    fn new(base: String) -> Self {
        Self {
            base,
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl market_data::hub::RawSourceAdapter for TestBtcturkAdapter {
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
                let url = format!("{}/api/v2/ticker?pairSymbol={}", self.base, symbol);
                let resp = self.client.get(&url).send()?;
                let json_v = resp.json::<Value>()?;
                if let Some(arr) = json_v.get("data").and_then(|d| d.as_array()) {
                    if let Some(item) = arr.get(0) {
                        let mut map = serde_json::Map::new();
                        if let Some(last) = item.get("last") {
                            map.insert("last".to_string(), last.clone());
                        }
                        if let Some(bid) = item.get("bid") {
                            map.insert("bid".to_string(), bid.clone());
                        }
                        if let Some(ask) = item.get("ask") {
                            map.insert("ask".to_string(), ask.clone());
                        }
                        if let Some(ts) = item.get("timestamp") {
                            map.insert("timestamp_ms".to_string(), ts.clone());
                        }
                        map.insert("source".to_string(), Value::String("btcturk".to_string()));
                        out.insert(ds.clone(), Value::Array(vec![Value::Object(map)]));
                    }
                }
            }
        }

        Ok(out)
    }
}

#[test]
fn btcturk_tick_fetch_via_mock() {
    let server = MockServer::start();

    let body = std::fs::read_to_string("tests/fixtures/btcturk_ticker.json").expect("read fixture");

    let _m = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v2/ticker")
            .query_param("pairSymbol", "BTCUSDT");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(body);
    });

    let mut registry = SourceAdapterRegistry::default();
    registry.register(
        "btcturk",
        Arc::new(TestBtcturkAdapter::new(server.base_url())),
    );

    let hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        registry,
        market_data::streaming::StreamingAdapterRegistry::default(),
    );

    let etl = Etl::new(hub)
        .source("btcturk")
        .select_assets(vec!["BTCUSDT".to_string()])
        .fetch(vec!["tick".to_string()])
        .expect("etl fetch should succeed");

    assert_eq!(etl.results().len(), 1);
    let result = &etl.results()[0];
    assert!(!result.records.is_empty());
    assert_eq!(result.records[0].source, "btcturk");
}
