use crate::hub::RawSourceAdapter;
use crate::providers::errors::ProviderError;
use serde_json::Value;
use std::collections::HashMap;

pub struct DovizComAdapter {
    client: reqwest::blocking::Client,
}

impl Default for DovizComAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl RawSourceAdapter for DovizComAdapter {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
    ) -> Result<HashMap<String, Value>, ProviderError> {
        let mut out = HashMap::new();

        let base = std::env::var("DOVIZCOM_BASE_URL").unwrap_or_else(|_| "https://www.doviz.com".to_string());
        let base = base.trim_end_matches('/');

        for ds in datasets {
            let canonical = crate::capabilities::canonical_dataset_name(ds);
            match canonical {
                "tick" => {
                    // Example endpoint that may exist: /api/v1/symbols/{symbol}/ticker
                    let url = format!("{}/api/v1/symbols/{}/ticker", base, symbol);
                    if let Ok(resp) = self.client.get(&url).send() {
                        if let Ok(json_v) = resp.json::<Value>() {
                            out.insert(canonical.to_string(), json_v);
                            continue;
                        }
                    }
                    out.insert(canonical.to_string(), Value::Array(Vec::new()));
                }
                _ => {}
            }
        }

        Ok(out)
    }
}
