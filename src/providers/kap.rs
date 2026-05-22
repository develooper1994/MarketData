use crate::hub::RawSourceAdapter;
use crate::providers::errors::ProviderError;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct KapAdapter {
    client: reqwest::blocking::Client,
}

impl Default for KapAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl RawSourceAdapter for KapAdapter {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
    ) -> Result<HashMap<String, Value>, ProviderError> {
        let mut out = HashMap::new();

        let base = std::env::var("KAP_BASE_URL").unwrap_or_else(|_| "https://www.kap.org.tr".to_string());
        let base = base.trim_end_matches('/');

        for ds in datasets {
            let canonical = crate::capabilities::canonical_dataset_name(ds);
            match canonical {
                "news" | "corporate_actions" | "fundamentals" => {
                    let url = format!("{}/tr/api/disclosures?company={}", base, symbol);
                    let resp = self.client.get(&url).send()?;
                    let mut json_v = resp.json::<Value>()?;
                    // Ensure each item has a `source` field so downstream consumers can rely on it
                    if let Some(arr) = json_v.as_array_mut() {
                        for item in arr.iter_mut() {
                            if let Value::Object(map) = item {
                                map.insert("source".to_string(), Value::String("kap".to_string()));
                            }
                        }
                    } else {
                        // if response isn't an array, wrap it
                        let wrapped = json!([json_v]);
                        json_v = wrapped;
                    }
                    out.insert(canonical.to_string(), json_v);
                }
                _ => {}
            }
        }

        Ok(out)
    }
}
