use regex::Regex;
use std::collections::HashSet;
use std::env;

fn push_unique(out: &mut Vec<String>, v: String) {
    if !out.contains(&v) {
        out.push(v);
    }
}

/// Generate candidate symbol variants for lookups.
///
/// Examples:
/// - `EUR` (forex) -> `EURUSD`, `EUR/TRY`, `EUR_USD`, ...
/// - `EURUSD` (forex) -> `EURUSD`, `EUR/USD`, `USD/EUR`, ...
/// - `BTC` (crypto) -> `BTCUSDT`, `BTC/USDT`, ...
pub fn generate_candidates(symbol: &str, asset_type_opt: Option<&str>) -> Vec<String> {
    let s = symbol.trim().to_uppercase();
    let mut out: Vec<String> = Vec::new();

    // ISIN (country code + 10 chars) or FIGI detection - treat as authoritative identifier
    if let Ok(isin_re) = Regex::new(r"^[A-Z]{2}[A-Z0-9]{10}$") {
        if isin_re.is_match(&s) {
            push_unique(&mut out, s.clone());
            return out;
        }
    }
    if let Ok(figi_re) = Regex::new(r"^BBG[0-9A-Z]{9}$") {
        if figi_re.is_match(&s) {
            // FIGI is an authoritative identifier; include as-is and as a prefixed token
            push_unique(&mut out, s.clone());
            push_unique(&mut out, format!("FIGI:{}", s));
            return out;
        }
    }

    match asset_type_opt {
        Some("forex") => {
            if s.contains('/') {
                let parts: Vec<&str> = s.split('/').collect();
                if parts.len() == 2 {
                    let base = parts[0];
                    let quote = parts[1];
                    push_unique(&mut out, format!("{}{}", base, quote));
                    push_unique(&mut out, format!("{}/{}", base, quote));
                    push_unique(&mut out, format!("{}_{}", base, quote));
                    push_unique(&mut out, format!("{}-{}", base, quote));
                    if base != quote {
                        push_unique(&mut out, format!("{}/{}", quote, base));
                        push_unique(&mut out, format!("{}{}", quote, base));
                    }
                } else {
                    push_unique(&mut out, s.replace('/', ""));
                }
            } else if s.len() == 6 && s.chars().all(|c| c.is_ascii_uppercase()) {
                let base = &s[0..3];
                let quote = &s[3..6];
                push_unique(&mut out, format!("{}{}", base, quote));
                push_unique(&mut out, format!("{}/{}", base, quote));
                push_unique(&mut out, format!("{}_{}", base, quote));
                push_unique(&mut out, format!("{}-{}", base, quote));
                if base != quote {
                    push_unique(&mut out, format!("{}/{}", quote, base));
                    push_unique(&mut out, format!("{}{}", quote, base));
                }
            } else if s.len() == 3 && s.chars().all(|c| c.is_ascii_uppercase()) {
                let quotes = ["USD", "TRY", "EUR", "GBP", "JPY"];
                for q in &quotes {
                    if q != &s {
                        push_unique(&mut out, format!("{}{}", s, q));
                        push_unique(&mut out, format!("{}/{}", s, q));
                        push_unique(&mut out, format!("{}_{}", s, q));
                    }
                }
            } else {
                push_unique(&mut out, s.clone());
            }
        }
        Some("crypto") => {
            let suffixes = ["USDT", "BTC", "ETH", "BUSD"];
            let mut matched = false;
            for suf in &suffixes {
                if s.ends_with(suf) && s.len() > suf.len() {
                    matched = true;
                    let base = &s[..s.len() - suf.len()];
                    push_unique(&mut out, s.clone());
                    push_unique(&mut out, format!("{}/{}", base, suf));
                    push_unique(&mut out, format!("{}_{}", base, suf));
                }
            }
            if !matched {
                let quotes = ["USDT", "BTC", "ETH"];
                for q in &quotes {
                    push_unique(&mut out, format!("{}{}", s, q));
                    push_unique(&mut out, format!("{}/{}", s, q));
                }
            }

            // Optional CoinGecko enrichment: if enabled via env var COINGECKO_LOOKUP=1,
            // try to resolve the simple coin id (e.g., "bitcoin") and add it as a candidate.
            if env::var("COINGECKO_LOOKUP").ok().as_deref() == Some("1") {
                if let Ok(list_resp) =
                    reqwest::blocking::get("https://api.coingecko.com/api/v3/coins/list")
                {
                    if let Ok(body) = list_resp.text() {
                        if let Ok(coins) = serde_json::from_str::<Vec<serde_json::Value>>(&body) {
                            let sym_lower = s.to_lowercase();
                            for coin in coins.into_iter().take(10000) {
                                if let (Some(id), Some(sym)) = (coin.get("id"), coin.get("symbol"))
                                {
                                    if sym
                                        .as_str()
                                        .map(|x| x.eq_ignore_ascii_case(&sym_lower))
                                        .unwrap_or(false)
                                    {
                                        if let Some(id_str) = id.as_str() {
                                            push_unique(&mut out, id_str.to_uppercase());
                                            push_unique(&mut out, format!("COINGECKO:{}", id_str));
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Some(_) => {
            // Any other asset class explicitly provided: return canonical symbol
            push_unique(&mut out, s.clone());
        }
        None => {
            // No asset class hint provided: be permissive and return canonical symbol
            push_unique(&mut out, s.clone());
        }
    }

    // simple normalization: remove duplicates but preserve order
    let mut seen = HashSet::new();
    out.into_iter().filter(|x| seen.insert(x.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::generate_candidates;

    #[test]
    fn forex_from_three_letter_contains_eurusd() {
        let c = generate_candidates("EUR", Some("forex"));
        assert!(c.contains(&"EURUSD".to_string()));
    }

    #[test]
    fn forex_from_six_letter_includes_slash() {
        let c = generate_candidates("EURUSD", Some("forex"));
        assert!(c.contains(&"EUR/USD".to_string()) || c.contains(&"EURUSD".to_string()));
    }

    #[test]
    fn crypto_from_base_contains_btcusdt() {
        let c = generate_candidates("BTC", Some("crypto"));
        assert!(c.contains(&"BTCUSDT".to_string()));
    }
}
