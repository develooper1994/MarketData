use httpmock::Method::GET;
use httpmock::MockServer;
use market_data::{
    DataHub, Etl, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry,
};
use serde_json::{Value, json};
use std::sync::Arc;

struct TestFintablesAdapter {
    base: String,
    client: reqwest::blocking::Client,
}

impl TestFintablesAdapter {
    fn new(base: String) -> Self {
        Self {
            base,
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl market_data::hub::RawSourceAdapter for TestFintablesAdapter {
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
            if ds == "corporate_actions" || ds == "fundamentals" {
                let url = format!(
                    "{}/sirketler/{}/sermaye-artirimlari-temettuler",
                    self.base, symbol
                );
                let resp = self.client.get(&url).send()?;
                let text = resp.text()?;
                out.insert(
                    ds.clone(),
                    json!([{"html": text, "source": "fintables", "symbol": symbol}]),
                );
            }
        }
        Ok(out)
    }
}

#[test]
fn fintables_fetch_via_mock() {
    let server = MockServer::start();
    let body =
        std::fs::read_to_string("tests/fixtures/fintables_example.html").expect("read fixture");
    let _m = server.mock(|when, then| {
        when.method(GET)
            .path("/sirketler/BTCUSDT/sermaye-artirimlari-temettuler");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(body);
    });

    let mut registry = SourceAdapterRegistry::default();
    registry.register(
        "fintables",
        Arc::new(TestFintablesAdapter::new(server.base_url())),
    );

    let hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        registry,
        market_data::streaming::StreamingAdapterRegistry::default(),
    );
    let etl = Etl::new(hub)
        .source("fintables")
        .select_assets(vec!["BTCUSDT".to_string()])
        .fetch(vec!["corporate_actions".to_string()])
        .expect("etl fetch should succeed");

    assert_eq!(etl.results().len(), 1);
    let result = &etl.results()[0];
    assert!(!result.records.is_empty());
    assert_eq!(result.records[0].source, "fintables");
}
