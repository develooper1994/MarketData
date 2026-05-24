#![cfg(feature = "async_providers")]

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

#[async_trait]
pub trait AsyncRawSourceAdapter: Send + Sync {
    async fn fetch_raw_async(
        &self,
        symbol: &str,
        datasets: &[String],
        timeframe: &str,
        limit: usize,
        requested_asset_class: Option<&str>,
        force_asset_class: bool,
    ) -> Result<HashMap<String, Value>, crate::providers::errors::ProviderError>;
}

pub struct YahooAsyncAdapter {
    client: reqwest::Client,
}

impl YahooAsyncAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl AsyncRawSourceAdapter for YahooAsyncAdapter {
    async fn fetch_raw_async(
        &self,
        symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
        _requested_asset_class: Option<&str>,
        _force_asset_class: bool,
    ) -> Result<HashMap<String, Value>, crate::providers::errors::ProviderError> {
        let mut out: HashMap<String, Value> = HashMap::new();
        let base = std::env::var("YAHOO_BASE_URL").unwrap_or_else(|_| "https://query1.finance.yahoo.com".to_string());
        let base = base.trim_end_matches('/');

        for ds in datasets {
            let canonical = crate::capabilities::canonical_dataset_name(ds);
            match canonical {
                "tick" => {
                    let url = format!("{}/v7/finance/quote?symbols={}", base, symbol);
                    let resp = self.client.get(&url).send().await?;
                    let json_v = resp.json::<Value>().await?;
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
                        record.insert("source".to_string(), Value::String("yahoo_async".to_string()));
                        out.insert(canonical.to_string(), Value::Array(vec![Value::Object(record)]));
                    }
                }
                _ => {}
            }
        }

        Ok(out)
    }
}
