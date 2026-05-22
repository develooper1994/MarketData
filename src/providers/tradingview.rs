use crate::hub::RawSourceAdapter;
use crate::providers::errors::ProviderError;
use serde_json::Value;
use std::collections::HashMap;

pub struct TradingViewAdapter {
    client: reqwest::blocking::Client,
}

impl Default for TradingViewAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl RawSourceAdapter for TradingViewAdapter {
    fn fetch_raw(
        &self,
        _symbol: &str,
        _datasets: &[String],
        _timeframe: &str,
        _limit: usize,
    ) -> Result<HashMap<String, Value>, ProviderError> {
        // Minimal scaffold: TradingView is primarily a JS/websocket driven source.
        // Implementations should be added later. For now return empty map.
        Ok(HashMap::new())
    }

    fn discover_assets(&self, _limit: usize) -> Vec<String> {
        // Best-effort: if a base URL is provided in env, attempt a scanner call; otherwise return empty.
        let base = std::env::var("TRADINGVIEW_SCANNER_URL").ok();
        if let Some(base) = base {
            let url = base.trim_end_matches('/').to_string();
            let body = r#"{"filter":[],"symbols":{"query":{"types":[]}},"columns":[]}"#;
            if let Ok(resp) = self.client.post(&url).body(body.to_string()).send() {
                if let Ok(json_v) = resp.json::<Value>() {
                    if let Some(arr) = json_v.get("data").and_then(|d| d.as_array()) {
                        let mut out = Vec::new();
                        for entry in arr.iter() {
                            if let Some(s) = entry.get("s").and_then(|v| v.as_str()) {
                                out.push(s.to_string());
                            }
                        }
                        return out;
                    }
                }
            }
        }

        Vec::new()
    }
}
