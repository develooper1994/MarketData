use crate::hub::RawSourceAdapter;
use crate::providers::errors::ProviderError;
use serde_json::Value;
use std::collections::HashMap;

pub struct YahooAdapter {
    client: reqwest::blocking::Client,
}

impl Default for YahooAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl RawSourceAdapter for YahooAdapter {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
        _requested_asset_class: Option<&str>,
        _force_asset_class: bool,
    ) -> Result<HashMap<String, Value>, ProviderError> {
        let mut out: HashMap<String, Value> = HashMap::new();

        // Allow tests to override base URL
        let base = std::env::var("YAHOO_BASE_URL")
            .unwrap_or_else(|_| "https://query1.finance.yahoo.com".to_string());
        let base = base.trim_end_matches('/');

        for ds in datasets {
            let canonical = crate::capabilities::canonical_dataset_name(ds);
            match canonical {
                "tick" => {
                    let url = format!("{}/v7/finance/quote?symbols={}", base, symbol);
                    let resp = self.client.get(&url).send()?;
                    let json_v = resp.json::<Value>()?;
                    if let Some(maybe) = json_v
                        .get("quoteResponse")
                        .and_then(|q| q.get("result"))
                        .and_then(|r| r.as_array())
                        .and_then(|arr| arr.first())
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
                        out.insert(
                            canonical.to_string(),
                            Value::Array(vec![Value::Object(record)]),
                        );
                    }
                }
                "kline" => {
                    let url = format!("{}/v8/finance/chart/{}?interval=1m&range=1d", base, symbol);
                    let resp = self.client.get(&url).send()?;
                    let json_v = resp.json::<Value>()?;
                    if let Some(res) = json_v
                        .get("chart")
                        .and_then(|c| c.get("result"))
                        .and_then(|r| r.as_array())
                        .and_then(|arr| arr.first())
                    {
                        if let (Some(ts_arr), Some(ind)) = (
                            res.get("timestamp"),
                            res.get("indicators")
                                .and_then(|i| i.get("quote"))
                                .and_then(|q| q.as_array())
                                .and_then(|arr| arr.first()),
                        ) {
                            if let (
                                Some(tss),
                                Some(open),
                                Some(high),
                                Some(low),
                                Some(close),
                                Some(volume),
                            ) = (
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
                                out.insert(canonical.to_string(), Value::Array(items));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(out)
    }
}
