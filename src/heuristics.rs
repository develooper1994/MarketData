use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct YAMLRules {
    rules: HashMap<String, Vec<String>>,
}

pub struct Heuristics {
    rules: HashMap<String, Vec<Regex>>,
}

impl Heuristics {
    fn load() -> Self {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = format!("{}/config/regex_rules.yaml", manifest);
        let mut rules_map: HashMap<String, Vec<Regex>> = HashMap::new();

        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(parsed) = serde_yaml::from_str::<YAMLRules>(&contents) {
                for (k, v) in parsed.rules.into_iter() {
                    let compiled = v
                        .into_iter()
                        .filter_map(|p| Regex::new(&p).ok())
                        .collect::<Vec<_>>();
                    rules_map.insert(k, compiled);
                }
            }
        }

        // Ensure sensible defaults if config missing keys
        if !rules_map.contains_key("forex") {
            rules_map.insert(
                "forex".to_string(),
                vec![
                    Regex::new(r"^[A-Z]{6}$").unwrap(),
                    Regex::new(r"^[A-Z]{3}/[A-Z]{3}$").unwrap(),
                ],
            );
        }
        if !rules_map.contains_key("crypto") {
            rules_map.insert(
                "crypto".to_string(),
                vec![Regex::new(r".*(USDT|BTC|ETH|BUSD)$").unwrap()],
            );
        }
        if !rules_map.contains_key("funds") {
            rules_map.insert("funds".to_string(), vec![Regex::new(r"^TRFUND").unwrap()]);
        }
        if !rules_map.contains_key("equities") {
            rules_map.insert(
                "equities".to_string(),
                vec![Regex::new(r"^[A-Z]{1,5}$").unwrap()],
            );
        }

        Heuristics { rules: rules_map }
    }

    pub fn detect(&self, symbol: &str) -> String {
        let s = symbol.trim().to_uppercase();

        // Ordered check: forex -> crypto -> funds -> equities
        for key in ["forex", "crypto", "funds", "equities"] {
            if let Some(vec) = self.rules.get(key) {
                for re in vec {
                    if re.is_match(&s) {
                        return key.to_string();
                    }
                }
            }
        }

        // fallback
        "equities".to_string()
    }
}

static DEFAULT: Lazy<Heuristics> = Lazy::new(|| Heuristics::load());

/// Detect asset type using the repository `config/regex_rules.yaml` if present.
pub fn detect_asset_type(symbol: &str) -> String {
    DEFAULT.detect(symbol)
}
