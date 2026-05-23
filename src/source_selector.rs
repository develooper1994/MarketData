use crate::heuristics;
use crate::source_registry::SourceRegistry;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Serialize)]
pub struct SelectionResult {
    pub chosen: Option<String>,
    pub candidates: Vec<String>,
    pub explain: serde_json::Value,
}

pub struct SourceSelector;

impl SourceSelector {
    pub fn select(
        symbol: &str,
        dataset: &str,
        registry: &SourceRegistry,
        asset_class_hint: Option<&str>,
        requested_source: Option<&str>,
        force_source: bool,
        force_asset_class: bool,
    ) -> SelectionResult {
        let symbol_norm = symbol.trim().to_uppercase();

        // If forced source requested, accept it immediately (if registered)
        if let Some(req) = requested_source {
            if force_source {
                return SelectionResult {
                    chosen: Some(req.to_string()),
                    candidates: vec![req.to_string()],
                    explain: json!({"reason":"force_source"}),
                };
            }
        }

        // Basic asset type detection (heuristic)
        let detected_asset_type = heuristics::detect_asset_type(&symbol_norm);
        // Normalize hint
        let hint_asset_type = asset_class_hint.map(|s| s.to_string());

        // Gather candidates that support the requested dataset
        let mut candidates: Vec<(&str, i32)> = Vec::new();

        for meta in registry.all() {
            if !meta.supported_datasets.iter().any(|d| d == dataset) {
                continue;
            }

            // base score on declared priority
            let mut score = meta.priority.unwrap_or(0) as i32;

            // If force_asset_class is set and a hint was provided, only include matching classes
            if force_asset_class {
                if let Some(ref h) = hint_asset_type {
                    if !meta.supported_asset_classes.iter().any(|c| c == h) {
                        continue;
                    }
                }
            }

            // prefer sources that explicitly support the hint asset type (if provided)
            if let Some(ref h) = hint_asset_type {
                if meta.supported_asset_classes.iter().any(|c| c == h) {
                    // strong bonus to push hinted-class sources first
                    score += 300;
                    if meta.supported_asset_classes.len() == 1 {
                        score += 50;
                    }
                }
            } else {
                // prefer sources that explicitly support the detected asset type
                if meta
                    .supported_asset_classes
                    .iter()
                    .any(|c| c == &detected_asset_type)
                {
                    score += 100;
                    if meta.supported_asset_classes.len() == 1 {
                        score += 50;
                    }
                }
            }

            candidates.push((meta.id.as_str(), score));
        }

        // sort candidates by score desc
        candidates.sort_by(|a, b| b.1.cmp(&a.1));

        let candidate_ids: Vec<String> = candidates.iter().map(|c| c.0.to_string()).collect();

        let chosen = candidates.first().map(|c| c.0.to_string());

        let explain = json!({
            "symbol": symbol_norm,
            "detected_asset_type": detected_asset_type,
            "asset_class_hint": asset_class_hint,
            "dataset": dataset,
            "candidates": candidates.iter().map(|(id, score)| json!({"id": id, "score": score})).collect::<Vec<_>>(),
        });

        SelectionResult {
            chosen,
            candidates: candidate_ids,
            explain,
        }
    }

    // Detection delegated to `heuristics::detect_asset_type`
}
