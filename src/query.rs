use crate::capabilities::{SourceCapability, canonical_dataset_name};
use std::collections::HashMap;

/// Returns "live", "partial", "api_key", "api_key_or_plan",
/// "metadata_only", or "unsupported" for a given source + dataset pair.
pub fn dataset_status_for_source(
    capabilities: &HashMap<String, SourceCapability>,
    source: &str,
    dataset: &str,
) -> String {
    let cap = match capabilities.get(source) {
        Some(c) => c,
        None => return "unsupported".to_string(),
    };
    let canonical = canonical_dataset_name(dataset);
    if !cap.datasets.iter().any(|d| d == canonical) {
        return "unsupported".to_string();
    }
    if cap.metadata_only_datasets.iter().any(|d| d == canonical) {
        return "metadata_only".to_string();
    }
    if !cap.implemented_datasets.iter().any(|d| d == canonical) {
        return "metadata_only".to_string();
    }
    cap.implementation_status.clone()
}

/// Returns "live", "partial", "unsupported", or the implementation_status
/// for a given source + asset class pair.
pub fn asset_status_for_source(
    capabilities: &HashMap<String, SourceCapability>,
    source: &str,
    asset_class: &str,
) -> String {
    let cap = match capabilities.get(source) {
        Some(c) => c,
        None => return "unsupported".to_string(),
    };
    if cap.asset_classes.iter().any(|a| a == asset_class) {
        cap.implementation_status.clone()
    } else {
        "unsupported".to_string()
    }
}

/// Returns all source names that support a given dataset and/or asset_class.
/// When `require_live` is true, only sources with `supports_realtime=true` are returned.
pub fn sources_for(
    capabilities: &HashMap<String, SourceCapability>,
    dataset: Option<&str>,
    asset_class: Option<&str>,
    require_live: bool,
) -> Vec<String> {
    let mut result: Vec<String> = capabilities
        .values()
        .filter(|cap| {
            if require_live && !cap.supports_realtime {
                return false;
            }
            if let Some(ds) = dataset {
                let canonical = canonical_dataset_name(ds);
                if !cap.implemented_datasets.iter().any(|d| d == canonical) {
                    return false;
                }
            }
            if let Some(ac) = asset_class
                && !cap.asset_classes.iter().any(|a| a == ac)
            {
                return false;
            }
            true
        })
        .map(|cap| cap.source.clone())
        .collect();
    result.sort();
    result
}

/// Returns implemented dataset names for a source.
pub fn available_datasets(
    capabilities: &HashMap<String, SourceCapability>,
    source: &str,
    implemented_only: bool,
) -> Vec<String> {
    let cap = match capabilities.get(source) {
        Some(c) => c,
        None => return vec![],
    };
    if implemented_only {
        cap.implemented_datasets.clone()
    } else {
        cap.datasets.clone()
    }
}

/// Returns sources ranked by suitability for a given use-case.
/// Factors: prefers live if `prefer_live`, filters api-key if `!allow_api_key`.
pub fn best_sources_for(
    capabilities: &HashMap<String, SourceCapability>,
    dataset: &str,
    asset_class: Option<&str>,
    prefer_live: bool,
    allow_api_key: bool,
    limit: Option<usize>,
) -> Vec<HashMap<String, String>> {
    let canonical = canonical_dataset_name(dataset);
    let mut sources: Vec<&SourceCapability> = capabilities
        .values()
        .filter(|cap| {
            if !allow_api_key && cap.requires_api_key {
                return false;
            }
            if !cap.implemented_datasets.iter().any(|d| d == canonical) {
                return false;
            }
            if let Some(ac) = asset_class
                && !cap.asset_classes.iter().any(|a| a == ac)
            {
                return false;
            }
            true
        })
        .collect();

    sources.sort_by(|a, b| {
        let live_score = |c: &SourceCapability| -> i32 {
            if prefer_live && c.supports_realtime {
                2
            } else {
                0
            }
        };
        let quality_score = |c: &SourceCapability| -> i32 {
            match c.quality_level.as_str() {
                "production" => 3,
                "best_effort" => 1,
                "fallback" => -1,
                _ => 0,
            }
        };
        let score_b = live_score(b) + quality_score(b);
        let score_a = live_score(a) + quality_score(a);
        score_b.cmp(&score_a).then(a.source.cmp(&b.source))
    });

    let iter: Box<dyn Iterator<Item = &SourceCapability>> = if let Some(n) = limit {
        Box::new(sources.into_iter().take(n))
    } else {
        Box::new(sources.into_iter())
    };

    iter.map(|cap| {
        let mut row = HashMap::new();
        row.insert("source".to_string(), cap.source.clone());
        row.insert("quality_level".to_string(), cap.quality_level.clone());
        row.insert(
            "implementation_status".to_string(),
            cap.implementation_status.clone(),
        );
        row.insert(
            "requires_api_key".to_string(),
            cap.requires_api_key.to_string(),
        );
        row
    })
    .collect()
}

/// Returns a brief summary of a source.
pub fn source_summary(
    capabilities: &HashMap<String, SourceCapability>,
    source: &str,
) -> HashMap<String, serde_json::Value> {
    use serde_json::json;
    let cap = match capabilities.get(source) {
        Some(c) => c,
        None => return HashMap::new(),
    };
    let mut m = HashMap::new();
    m.insert("source".to_string(), json!(cap.source));
    m.insert("asset_classes".to_string(), json!(cap.asset_classes));
    m.insert("datasets".to_string(), json!(cap.datasets));
    m.insert("quality_level".to_string(), json!(cap.quality_level));
    m.insert(
        "implementation_status".to_string(),
        json!(cap.implementation_status),
    );
    m.insert("requires_api_key".to_string(), json!(cap.requires_api_key));
    m.insert(
        "supports_realtime".to_string(),
        json!(cap.supports_realtime),
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::capability_map;

    #[test]
    fn dataset_status_known_live_source() {
        let caps = capability_map();
        assert_eq!(
            dataset_status_for_source(&caps, "binance_futures", "kline"),
            "live"
        );
    }

    #[test]
    fn dataset_status_unsupported_dataset() {
        let caps = capability_map();
        assert_eq!(
            dataset_status_for_source(&caps, "stooq", "orderbook"),
            "unsupported"
        );
    }

    #[test]
    fn dataset_status_alias_resolved() {
        let caps = capability_map();
        assert_eq!(
            dataset_status_for_source(&caps, "binance_futures", "ohlcv"),
            "live"
        );
    }

    #[test]
    fn sources_for_kline() {
        let caps = capability_map();
        let result = sources_for(&caps, Some("kline"), None, false);
        assert!(result.contains(&"binance_futures".to_string()));
        assert!(result.contains(&"stooq".to_string()));
    }

    #[test]
    fn sources_for_require_live() {
        let caps = capability_map();
        let result = sources_for(&caps, Some("kline"), None, true);
        assert!(result.contains(&"binance_futures".to_string()));
        assert!(!result.contains(&"stooq".to_string()));
    }

    #[test]
    fn best_sources_returns_ranked_list() {
        let caps = capability_map();
        let result = best_sources_for(&caps, "kline", None, true, true, Some(3));
        assert!(!result.is_empty());
        assert!(result[0].contains_key("source"));
    }
}
