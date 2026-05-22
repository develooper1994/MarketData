use httpmock::Method::GET;
use httpmock::MockServer;
use market_data::{DataHub, Etl, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry};
use serde_json::Value;
use std::sync::Arc;

struct TestYahooAdapter {
    base: String,
    client: reqwest::blocking::Client,
}

impl TestYahooAdapter {
    fn new(base: String) -> Self {
        Self {
            base,
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl market_data::hub::RawSourceAdapter for TestYahooAdapter {
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
                let url = format!("{}/v7/finance/quote?symbols={}", self.base, symbol);
                let resp = self.client.get(&url).send()?;
                let json_v = resp.json::<Value>()?;
                if let Some(maybe) = json_v
                    .get("quoteResponse")
                    .and_then(|q| q.get("result"))
                    .and_then(|r| r.as_array())
                    .and_then(|arr| arr.get(0))
                    .cloned()
                {
                    let mut record = serde_json::Map::new();
                    if let Some(price) = maybe.get("regularMarketPrice") {
                        record.insert("last".to_string(), price.clone());
                    }
                    if let Some(timev) = maybe.get("regularMarketTime") {
                        if let Some(ts) = timev.as_i64() {
                            record.insert("timestamp_ms".to_string(), Value::from(ts * 1000));
                        }
                    }
                    record.insert("source".to_string(), Value::String("yahoo".to_string()));
                    out.insert(ds.clone(), Value::Array(vec![Value::Object(record)]));
                }
            } else if ds == "kline" {
                let url = format!("{}/v8/finance/chart/{}?interval=1m&range=1d", self.base, symbol);
                let resp = self.client.get(&url).send()?;
                let json_v = resp.json::<Value>()?;
                if let Some(res) = json_v.get("chart").and_then(|c| c.get("result")).and_then(|r| r.get(0)) {
                    if let (Some(ts_arr), Some(ind)) = (
                        res.get("timestamp"),
                        res.get("indicators").and_then(|i| i.get("quote")).and_then(|q| q.get(0)),
                    ) {
                        if let (Some(tss), Some(open), Some(high), Some(low), Some(close), Some(volume)) = (
                            ts_arr.as_array(),
                            ind.get("open").and_then(|v| v.as_array()),
                            ind.get("high").and_then(|v| v.as_array()),
                            ind.get("low").and_then(|v| v.as_array()),
                            ind.get("close").and_then(|v| v.as_array()),
                            ind.get("volume").and_then(|v| v.as_array()),
                        ) {
                            let mut items = Vec::new();
                            let count = std::cmp::min(tss.len(), close.len());
                            for i in 0..count {
                                let mut row = Vec::new();
                                let ts = tss[i].as_i64().unwrap_or(0) * 1000;
                                row.push(Value::from(ts));
                                row.push(open.get(i).cloned().unwrap_or(Value::Null));
                                row.push(high.get(i).cloned().unwrap_or(Value::Null));
                                row.push(low.get(i).cloned().unwrap_or(Value::Null));
                                row.push(close.get(i).cloned().unwrap_or(Value::Null));
                                row.push(volume.get(i).cloned().unwrap_or(Value::Null));
                                items.push(Value::Array(row));
                            }
                            out.insert(ds.clone(), Value::Array(items));
                        }
                    }
                }
            }
        }

        Ok(out)
    }
}

#[test]
fn yahoo_tick_fetch_via_mock() {
    let server = MockServer::start();

    let quote_body = std::fs::read_to_string("tests/fixtures/yahoo_quote.json").expect("read fixture");

    let _m = server.mock(|when, then| {
        when.method(GET)
            .path("/v7/finance/quote")
            .query_param("symbols", "BTCUSDT");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(quote_body);
    });

    let mut registry = SourceAdapterRegistry::default();
    registry.register(
        "yahoo",
        Arc::new(TestYahooAdapter::new(server.base_url())),
    );

    let hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        registry,
    );

    let etl = Etl::new(hub)
        .source("yahoo")
        .select_assets(vec!["BTCUSDT".to_string()])
        .fetch(vec!["tick".to_string()])
        .expect("etl fetch should succeed");

    assert_eq!(etl.results().len(), 1);
    let result = &etl.results()[0];
    assert!(!result.records.is_empty());
    assert_eq!(result.records[0].source, "yahoo");
}

#[test]
fn yahoo_chart_fetch_via_mock() {
    let server = MockServer::start();

    let chart_body = std::fs::read_to_string("tests/fixtures/yahoo_chart.json").expect("read fixture");

    let _m = server.mock(|when, then| {
        when.method(GET)
            .path("/v8/finance/chart/BTCUSDT")
            .query_param("interval", "1m")
            .query_param("range", "1d");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(chart_body);
    });

    let mut registry = SourceAdapterRegistry::default();
    registry.register(
        "yahoo",
        Arc::new(TestYahooAdapter::new(server.base_url())),
    );

    let hub = DataHub::with_components(
        Box::new(InMemoryStorage::default()),
        ManifestProvenanceTracker::new(None::<&str>),
        registry,
    );

    let etl = Etl::new(hub)
        .source("yahoo")
        .select_assets(vec!["BTCUSDT".to_string()])
        .fetch(vec!["kline".to_string()])
        .expect("etl fetch should succeed");

    assert_eq!(etl.results().len(), 1);
    let result = &etl.results()[0];
    assert!(!result.records.is_empty());
    assert_eq!(result.records[0].source, "yahoo");
}
