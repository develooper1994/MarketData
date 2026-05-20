use crate::contracts::{DataRecord, QualityReport};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub struct CanonicalDataQuality;

impl CanonicalDataQuality {
    pub fn validate(&self, records: &[DataRecord]) -> QualityReport {
        let checks = vec![
            "records_present".to_string(),
            "required_fields".to_string(),
            "monotonic_timestamp".to_string(),
            "duplicate_check".to_string(),
            "non_null_ohlcv".to_string(),
            "non_negative_values".to_string(),
        ];

        if records.is_empty() {
            return QualityReport {
                passed: false,
                checks,
                issues: vec!["No records fetched for request.".to_string()],
            };
        }

        let mut issues = Vec::new();
        let mut seen_keys = BTreeSet::new();
        let mut grouped_timestamps: BTreeMap<(String, String), Vec<i64>> = BTreeMap::new();

        for (index, record) in records.iter().enumerate() {
            if record.key.is_empty()
                || record.observed_at.is_empty()
                || record.source.is_empty()
                || record.asset_type.is_empty()
            {
                issues.push(format!("Record {index} missing required metadata."));
            }
            if !seen_keys.insert(record.key.clone()) {
                issues.push(format!("Duplicate record key detected: {}", record.key));
            }

            let dataset = record
                .metadata
                .get("dataset")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let join_key = record
                .metadata
                .get("join_key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let timestamp_ms = record
                .payload
                .get("timestamp_ms")
                .and_then(Value::as_i64)
                .unwrap_or_default();

            if timestamp_ms > 0 {
                grouped_timestamps
                    .entry((dataset.clone(), join_key.clone()))
                    .or_default()
                    .push(timestamp_ms);
            }

            if dataset == "kline" {
                for field in ["open", "high", "low", "close", "volume"] {
                    if !record.payload.contains_key(field) {
                        issues.push(format!("OHLCV record missing {field}: {}", record.key));
                    }
                }
                check_non_negative(&record.payload, &record.key, "open", &mut issues);
                check_non_negative(&record.payload, &record.key, "high", &mut issues);
                check_non_negative(&record.payload, &record.key, "low", &mut issues);
                check_non_negative(&record.payload, &record.key, "close", &mut issues);
                check_non_negative(&record.payload, &record.key, "volume", &mut issues);
            }
        }

        for ((dataset, join_key), timestamps) in grouped_timestamps {
            if timestamps
                != timestamps
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
            {
                issues.push(format!(
                    "Duplicate timestamp for dataset={dataset} join_key={join_key}"
                ));
            }
            let mut sorted = timestamps.clone();
            sorted.sort_unstable();
            if sorted != timestamps {
                issues.push(format!(
                    "Non-monotonic timestamps for dataset={dataset} join_key={join_key}"
                ));
            }
        }

        QualityReport {
            passed: issues.is_empty(),
            checks,
            issues,
        }
    }
}

fn check_non_negative(
    payload: &BTreeMap<String, Value>,
    record_key: &str,
    field: &str,
    issues: &mut Vec<String>,
) {
    let value = payload
        .get(field)
        .and_then(Value::as_f64)
        .unwrap_or_default();
    if value < 0.0 {
        issues.push(format!("Negative {field} in {record_key}"));
    }
}
