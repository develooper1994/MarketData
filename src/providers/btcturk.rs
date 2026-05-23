use crate::hub::RawSourceAdapter;
use crate::providers::errors::ProviderError;
use serde_json::Value;
use std::collections::HashMap;

pub struct BtcturkAdapter {
    client: reqwest::blocking::Client,
}

impl Default for BtcturkAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl RawSourceAdapter for BtcturkAdapter {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
        _requested_asset_class: Option<&str>,
        _force_asset_class: bool,
    ) -> Result<HashMap<String, Value>, ProviderError> {
        let mut out = HashMap::new();

        let base = std::env::var("BTCTURK_BASE_URL")
            .unwrap_or_else(|_| "https://api.btcturk.com".to_string());
        let base = base.trim_end_matches('/');

        for ds in datasets {
            let canonical = crate::capabilities::canonical_dataset_name(ds);
            match canonical {
                "tick" => {
                    let url = format!("{}/api/v2/ticker?pairSymbol={}", base, symbol);
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
                            out.insert(
                                canonical.to_string(),
                                Value::Array(vec![Value::Object(map)]),
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(out)
    }
}
