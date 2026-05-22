use crate::hub::RawSourceAdapter;
use crate::providers::errors::ProviderError;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct FintablesAdapter {
    client: reqwest::blocking::Client,
}

impl Default for FintablesAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl RawSourceAdapter for FintablesAdapter {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
    ) -> Result<HashMap<String, Value>, ProviderError> {
        let mut out = HashMap::new();

        // Fintables scraping is opt-in via ENABLE_SCRAPING_PROVIDERS env var
        let scraping_enabled = std::env::var("ENABLE_SCRAPING_PROVIDERS").unwrap_or_default() == "true";

        let base = std::env::var("FINTABLES_BASE_URL").unwrap_or_else(|_| "https://fintables.com".to_string());
        let base = base.trim_end_matches('/');

        for ds in datasets {
            let canonical = crate::capabilities::canonical_dataset_name(ds);
            if canonical == "corporate_actions" || canonical == "fundamentals" {
                if !scraping_enabled {
                    // return empty so hub will fallback
                    continue;
                }

                // Example URL: https://fintables.com/sirketler/{ticker}/sermaye-artirimlari-temettuler
                let url = format!(
                    "{}/sirketler/{}/sermaye-artirimlari-temettuler",
                    base, symbol
                );
                let resp = self.client.get(&url).send()?;
                let text = resp.text()?;
                // Return raw HTML inside a single record so parsers/tests can operate on it
                out.insert(
                    canonical.to_string(),
                    json!([{"html": text, "source": "fintables", "symbol": symbol}]),
                );
            }
        }

        Ok(out)
    }
}
