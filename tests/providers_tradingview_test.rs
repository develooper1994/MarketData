use httpmock::Method::POST;
use httpmock::MockServer;
use market_data::{
    DataHub, Etl, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry,
};
use serde_json::Value;
use std::sync::Arc;

struct TestTradingViewAdapter {
    base: String,
    client: reqwest::blocking::Client,
}

impl TestTradingViewAdapter {
    fn new(base: String) -> Self {
        Self {
            base,
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl market_data::hub::RawSourceAdapter for TestTradingViewAdapter {
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
            if ds == "tick" {
                let url = format!("{}/scan", self.base);
                let body = serde_json::json!({"query": {"symbol": symbol}}).to_string();
                let resp = self.client.post(&url).body(body).send()?;
                let json_v = resp.json::<Value>()?;
                if let Some(arr) = json_v.get("data").and_then(|d| d.as_array()) {
                    if let Some(item) = arr.get(0) {
                        let mut map = serde_json::Map::new();
                        if let Some(last) = item.get("last") {
                            map.insert("last".to_string(), last.clone());
                        }
                        map.insert(
                            "source".to_string(),
                            Value::String("tradingview".to_string()),
                        );
                        out.insert(ds.clone(), Value::Array(vec![Value::Object(map)]));
                    }
                }
            }
        }
        Ok(out)
    }
}

#[test]
fn tradingview_tick_fetch_via_mock() {
    let server = MockServer::start();

    let body_json = serde_json::json!({"data": [{"s": "BTCUSDT", "last": 123.45}]});

    let _m = server.mock(|when, then| {
        when.method(POST).path("/scan");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(body_json.to_string());
    });

    let mut registry = SourceAdapterRegistry::default();
    registry.register(
        "tradingview",
        Arc::new(TestTradingViewAdapter::new(server.base_url())),
    );

    let hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        registry,
        market_data::streaming::StreamingAdapterRegistry::default(),
    );

    let etl = Etl::new(hub)
        .source("tradingview")
        .select_assets(vec!["BTCUSDT".to_string()])
        .fetch(vec!["tick".to_string()])
        .expect("etl fetch should succeed");

    assert_eq!(etl.results().len(), 1);
    let result = &etl.results()[0];
    assert!(!result.records.is_empty());
    assert_eq!(result.records[0].source, "tradingview");
}
