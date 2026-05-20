use crate::contracts::DataRecord;
use chrono::{TimeZone, Utc};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub fn normalize_dataset(
    dataset: &str,
    source: &str,
    symbol: &str,
    raw_payload: &Value,
) -> Vec<BTreeMap<String, Value>> {
    if dataset == "kline" {
        return normalize_kline(source, symbol, raw_payload);
    }

    match raw_payload {
        Value::Array(items) => items
            .iter()
            .filter_map(value_to_object)
            .map(map_to_btreemap)
            .collect(),
        Value::Object(_) => value_to_object(raw_payload)
            .map(map_to_btreemap)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

pub fn to_data_records(
    dataset: &str,
    source: &str,
    asset_type: &str,
    symbol: &str,
    items: &[BTreeMap<String, Value>],
) -> Vec<DataRecord> {
    items
        .iter()
        .map(|payload| {
            let timestamp_ms = payload
                .get("timestamp_ms")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            let observed_at = Utc
                .timestamp_millis_opt(timestamp_ms)
                .single()
                .unwrap_or_else(Utc::now)
                .to_rfc3339();
            let key = format!("{source}:{dataset}:{symbol}:{timestamp_ms}");

            let mut metadata = BTreeMap::new();
            metadata.insert("dataset".to_string(), Value::String(dataset.to_string()));
            metadata.insert("join_key".to_string(), Value::String(symbol.to_string()));

            DataRecord {
                key,
                observed_at,
                domain: "market_data".to_string(),
                source: source.to_string(),
                asset_type: asset_type.to_string(),
                payload: payload.clone(),
                metadata,
            }
        })
        .collect()
}

fn normalize_kline(
    _source: &str,
    _symbol: &str,
    raw_payload: &Value,
) -> Vec<BTreeMap<String, Value>> {
    let mut out = Vec::new();
    let items = match raw_payload {
        Value::Array(items) => items,
        _ => return out,
    };

    for item in items {
        let mut row = BTreeMap::new();
        match item {
            Value::Array(values) if values.len() >= 6 => {
                row.insert("timestamp_ms".to_string(), values[0].clone());
                row.insert("open".to_string(), to_number_value(&values[1]));
                row.insert("high".to_string(), to_number_value(&values[2]));
                row.insert("low".to_string(), to_number_value(&values[3]));
                row.insert("close".to_string(), to_number_value(&values[4]));
                row.insert("volume".to_string(), to_number_value(&values[5]));
            }
            Value::Object(map) => {
                row.insert(
                    "timestamp_ms".to_string(),
                    map.get("timestamp_ms")
                        .or_else(|| map.get("t"))
                        .cloned()
                        .unwrap_or(Value::from(0_i64)),
                );
                row.insert(
                    "open".to_string(),
                    to_number_value(map.get("open").unwrap_or(&Value::Null)),
                );
                row.insert(
                    "high".to_string(),
                    to_number_value(map.get("high").unwrap_or(&Value::Null)),
                );
                row.insert(
                    "low".to_string(),
                    to_number_value(map.get("low").unwrap_or(&Value::Null)),
                );
                row.insert(
                    "close".to_string(),
                    to_number_value(map.get("close").unwrap_or(&Value::Null)),
                );
                row.insert(
                    "volume".to_string(),
                    to_number_value(map.get("volume").unwrap_or(&Value::Null)),
                );
            }
            _ => continue,
        }
        out.push(row);
    }

    out
}

fn to_number_value(value: &Value) -> Value {
    match value {
        Value::Number(_) => value.clone(),
        Value::String(raw) => raw
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn value_to_object(value: &Value) -> Option<Map<String, Value>> {
    match value {
        Value::Object(map) => Some(map.clone()),
        _ => None,
    }
}

fn map_to_btreemap(map: Map<String, Value>) -> BTreeMap<String, Value> {
    map.into_iter().collect()
}
