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
    match dataset {
        "kline" => normalize_kline(source, symbol, raw_payload),
        "tick" => normalize_tick(raw_payload),
        "trade" => normalize_trade(raw_payload),
        "orderbook" => normalize_orderbook(raw_payload),
        "funding" => normalize_funding(raw_payload),
        "macro" => normalize_generic_records(raw_payload),
        "news" => normalize_generic_records(raw_payload),
        "fundamentals" => normalize_generic_records(raw_payload),
        "corporate_actions" => normalize_generic_records(raw_payload),
        _ => normalize_passthrough(raw_payload),
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
        .enumerate()
        .map(|(index, payload)| {
            let timestamp_ms = payload
                .get("timestamp_ms")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            let observed_at = Utc
                .timestamp_millis_opt(timestamp_ms)
                .single()
                .unwrap_or_else(Utc::now)
                .to_rfc3339();
            let key = format!("{source}:{dataset}:{symbol}:{timestamp_ms}:{}", index + 1);

            let mut metadata = BTreeMap::new();
            metadata.insert("dataset".to_string(), Value::String(dataset.to_string()));
            metadata.insert("join_key".to_string(), Value::String(symbol.to_string()));

            DataRecord {
                key,
                observed_at,
                domain: dataset_domain(dataset).to_string(),
                source: source.to_string(),
                asset_type: asset_type.to_string(),
                payload: payload.clone(),
                metadata,
            }
        })
        .collect()
}

fn dataset_domain(dataset: &str) -> &str {
    match dataset {
        "kline" | "trade" | "orderbook" | "funding" | "tick" => "market",
        "news" => "news",
        "macro" => "macro",
        "fundamentals" | "corporate_actions" => "fundamentals",
        _ => dataset,
    }
}

// ---------------------------------------------------------------------------
// Kline (OHLCV)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Tick (best bid/ask snapshot)
// ---------------------------------------------------------------------------

fn normalize_tick(raw_payload: &Value) -> Vec<BTreeMap<String, Value>> {
    let items = collect_records(raw_payload);
    items
        .into_iter()
        .map(|map| {
            let mut row = BTreeMap::new();
            let ts = map
                .get("timestamp_ms")
                .or_else(|| map.get("t"))
                .or_else(|| map.get("time"))
                .cloned()
                .unwrap_or(Value::from(0_i64));
            row.insert("timestamp_ms".to_string(), coerce_timestamp_ms(&ts));
            for field in ["bid", "ask", "last", "price", "volume"] {
                if let Some(v) = map.get(field) {
                    row.insert(field.to_string(), to_number_value(v));
                }
            }
            row
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Trade
// ---------------------------------------------------------------------------

fn normalize_trade(raw_payload: &Value) -> Vec<BTreeMap<String, Value>> {
    let items = collect_records(raw_payload);
    items
        .into_iter()
        .map(|map| {
            let mut row = BTreeMap::new();
            let ts = map
                .get("timestamp_ms")
                .or_else(|| map.get("t"))
                .or_else(|| map.get("time"))
                .cloned()
                .unwrap_or(Value::from(0_i64));
            row.insert("timestamp_ms".to_string(), coerce_timestamp_ms(&ts));
            for field in ["price", "qty", "quantity", "side", "id"] {
                if let Some(v) = map.get(field) {
                    row.insert(field.to_string(), v.clone());
                }
            }
            row
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Orderbook
// ---------------------------------------------------------------------------

fn normalize_orderbook(raw_payload: &Value) -> Vec<BTreeMap<String, Value>> {
    // Orderbook payloads vary greatly; pass through as a single record.
    match raw_payload {
        Value::Object(map) => {
            let row: BTreeMap<String, Value> =
                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            vec![row]
        }
        Value::Array(_) => normalize_passthrough(raw_payload),
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// Funding rate
// ---------------------------------------------------------------------------

fn normalize_funding(raw_payload: &Value) -> Vec<BTreeMap<String, Value>> {
    let items = collect_records(raw_payload);
    items
        .into_iter()
        .map(|map| {
            let mut row = BTreeMap::new();
            let ts = map
                .get("timestamp_ms")
                .or_else(|| map.get("fundingTime"))
                .or_else(|| map.get("t"))
                .cloned()
                .unwrap_or(Value::from(0_i64));
            row.insert("timestamp_ms".to_string(), coerce_timestamp_ms(&ts));
            for field in ["fundingRate", "funding_rate", "rate"] {
                if let Some(v) = map.get(field) {
                    row.insert("funding_rate".to_string(), to_number_value(v));
                    break;
                }
            }
            row
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Generic passthrough for macro / news / fundamentals / corporate_actions
// ---------------------------------------------------------------------------

fn normalize_generic_records(raw_payload: &Value) -> Vec<BTreeMap<String, Value>> {
    let items = collect_records(raw_payload);
    items
        .into_iter()
        .map(|map| {
            let mut row: BTreeMap<String, Value> = map.into_iter().collect();
            // Inject timestamp_ms if a recognisable date field is present but
            // timestamp_ms is missing.
            if !row.contains_key("timestamp_ms") {
                let date_str = row
                    .get("date")
                    .or_else(|| row.get("datetime"))
                    .or_else(|| row.get("publishedAt"))
                    .or_else(|| row.get("datetime_utc"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                if let Some(ts_ms) = date_str.and_then(parse_rfc3339_to_ms) {
                    row.insert("timestamp_ms".to_string(), Value::from(ts_ms));
                }
            }
            row
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Fallback passthrough (preserves raw structure as-is)
// ---------------------------------------------------------------------------

fn normalize_passthrough(raw_payload: &Value) -> Vec<BTreeMap<String, Value>> {
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn collect_records(raw_payload: &Value) -> Vec<Map<String, Value>> {
    match raw_payload {
        Value::Array(items) => items.iter().filter_map(value_to_object).collect(),
        Value::Object(_) => value_to_object(raw_payload).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn coerce_timestamp_ms(value: &Value) -> Value {
    match value {
        Value::Number(_) => value.clone(),
        Value::String(s) => s
            .parse::<i64>()
            .ok()
            .map(Value::from)
            .unwrap_or(Value::from(0_i64)),
        _ => Value::from(0_i64),
    }
}

pub(crate) fn to_number_value(value: &Value) -> Value {
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

/// Parse an RFC 3339 / ISO 8601 date-time string and return epoch milliseconds.
fn parse_rfc3339_to_ms(date_str: String) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(&date_str)
        .ok()
        .map(|ts| ts.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_tick_extracts_fields() {
        let raw = json!([{"timestamp_ms": 1716200000000_i64, "bid": "10.5", "ask": "10.6", "last": "10.55"}]);
        let rows = normalize_dataset("tick", "test", "BTCUSDT", &raw);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains_key("bid"));
        assert!(rows[0].contains_key("ask"));
    }

    #[test]
    fn normalize_trade_extracts_fields() {
        let raw = json!([{"t": 1716200000000_i64, "price": "100.0", "qty": "1.5", "side": "buy"}]);
        let rows = normalize_dataset("trade", "test", "BTCUSDT", &raw);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains_key("price"));
    }

    #[test]
    fn normalize_funding_extracts_rate() {
        let raw = json!([{"fundingTime": 1716200000000_i64, "fundingRate": "0.0001"}]);
        let rows = normalize_dataset("funding", "test", "BTCUSDT", &raw);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains_key("funding_rate"));
    }

    #[test]
    fn normalize_macro_passes_through_with_timestamp() {
        let raw =
            json!([{"date": "2024-01-01T00:00:00Z", "value": "5.5", "series_id": "FEDFUNDS"}]);
        let rows = normalize_dataset("macro", "fred", "FEDFUNDS", &raw);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains_key("timestamp_ms"));
    }

    #[test]
    fn normalize_news_preserves_fields() {
        let raw = json!([{"title": "Test story", "url": "https://example.com", "publishedAt": "2024-01-01T00:00:00Z"}]);
        let rows = normalize_dataset("news", "gdelt", "AAPL", &raw);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains_key("title"));
    }

    #[test]
    fn normalize_unknown_dataset_passes_through() {
        let raw = json!([{"foo": "bar"}]);
        let rows = normalize_dataset("custom_dataset", "test", "SYM", &raw);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["foo"], json!("bar"));
    }
}
