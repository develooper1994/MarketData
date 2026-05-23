use crate::hub::RawSourceAdapter;
use crate::providers::errors::ProviderError;
use serde_json::{Value, json};
use std::collections::HashMap;
use chrono::{NaiveDate, Utc, TimeZone};
use tefas::{AppConfig, TefasClient, QueryBatchRequest, DEFAULT_USER_AGENT};
use tefas::QueryOperationName;
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

// Shared runtime to avoid creating a new Runtime per call which is expensive
static SHARED_RT: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("failed to create tokio runtime"));

pub struct TefasAdapter;

impl Default for TefasAdapter {
    fn default() -> Self {
        Self {}
    }
}

impl TefasAdapter {
    fn build_client(&self) -> Result<TefasClient, ProviderError> {
        let mut cfg = AppConfig::with_defaults();
        if let Ok(env_base) = std::env::var("TEFAS_BASE_URL") {
            if !env_base.trim().is_empty() { cfg.base_url = env_base; }
        }
        // Keep CLI-like UA for compatibility with TEFAS WAF behaviour
        cfg.auth.user_agent = DEFAULT_USER_AGENT.to_string();

        let rt = &*SHARED_RT;
        rt.block_on(async { TefasClient::new(cfg).map_err(|e| ProviderError::Other(format!("tefas client init: {}", e))) })
    }

    fn query_fon_fiyat(&self, client: &TefasClient, symbol: &str) -> Result<Value, ProviderError> {
        let rt = &*SHARED_RT;
        let req = QueryBatchRequest::new(vec![QueryOperationName::new("fonFiyatBilgiGetir")], None);
        let overrides = vec![("fonKodu".to_string(), Value::String(symbol.to_string())), ("periyod".to_string(), Value::String("1".to_string()))];
        rt.block_on(async { client.query_by_names(req, overrides, None).await.map_err(|e| ProviderError::Other(format!("tefas query error: {}", e))) })
    }
}

impl RawSourceAdapter for TefasAdapter {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
    ) -> Result<HashMap<String, Value>, ProviderError> {
        let mut out = HashMap::new();

        let tefas_debug = std::env::var("TEFAS_DEBUG").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
        let write_parity = |symbol: &str, kind: &str, val: &Value| {
            if tefas_debug {
                let dir = format!("artifacts/tefas_parity/{}", symbol);
                let _ = std::fs::create_dir_all(&dir);
                let fname = format!("{}/{}.json", dir, kind);
                if let Ok(body) = serde_json::to_string_pretty(val) {
                    let _ = std::fs::write(&fname, body);
                }
            }
        };

        // Diagnostic marker when debugging enabled
        if tefas_debug {
            let dir = format!("artifacts/tefas_parity/{}", symbol);
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(format!("{}/invoked.txt", dir), "adapter_invoked");
        }

        // Build a single client for all dataset queries
        let client = match self.build_client() {
            Ok(c) => c,
            Err(e) => {
                if tefas_debug {
                    let dir = format!("artifacts/tefas_parity/{}", symbol);
                    let _ = std::fs::create_dir_all(&dir);
                    let fname = format!("{}/client_error.txt", dir);
                    let _ = std::fs::write(&fname, format!("{}", e));
                }
                return Err(e);
            }
        };

        for ds in datasets {
            let canonical = crate::capabilities::canonical_dataset_name(ds);
            match canonical {
                // For fund-related datasets, query the `fonFiyatBilgiGetir` operation
                "kline" => {
                    let v = match self.query_fon_fiyat(&client, symbol) {
                        Ok(v) => v,
                        Err(e) => {
                            if tefas_debug {
                                let dir = format!("artifacts/tefas_parity/{}", symbol);
                                let _ = std::fs::create_dir_all(&dir);
                                let fname = format!("{}/lib_error.txt", dir);
                                let _ = std::fs::write(&fname, format!("{}", e));
                            }
                            out.insert(canonical.to_string(), json!([]));
                            continue;
                        }
                    };
                    write_parity(symbol, "lib_raw", &v);
                    if let Some(arr) = v.get("fonFiyatBilgiGetir").and_then(|o| o.get("resultList")).and_then(|r| r.as_array()) {
                        let mut out_arr = Vec::new();
                        for item in arr {
                            if let Some(obj) = item.as_object() {
                                let tarih = obj.get("tarih").and_then(|v| v.as_str()).unwrap_or_default();
                                let ts_ms = NaiveDate::parse_from_str(tarih, "%Y-%m-%d").ok().map(|d| d.and_hms_opt(0, 0, 0).unwrap()).map(|ndt| Utc.from_utc_datetime(&ndt).timestamp_millis()).unwrap_or(0_i64);
                                let price = obj.get("fiyat").cloned().or_else(|| obj.get("price").cloned()).unwrap_or(Value::Null);
                                let mut rec = serde_json::Map::new();
                                rec.insert("timestamp_ms".to_string(), Value::from(ts_ms));
                                rec.insert("open".to_string(), price.clone());
                                rec.insert("high".to_string(), price.clone());
                                rec.insert("low".to_string(), price.clone());
                                rec.insert("close".to_string(), price.clone());
                                rec.insert("volume".to_string(), Value::Null);
                                out_arr.push(Value::Object(rec));
                            }
                        }
                        out.insert(canonical.to_string(), Value::Array(out_arr));
                    } else {
                        // no resultList -> return empty dataset (DataHub will mark missing)
                        out.insert(canonical.to_string(), json!([]));
                    }
                }

                "tick" => {
                    let v = match self.query_fon_fiyat(&client, symbol) {
                        Ok(v) => v,
                        Err(e) => {
                            if tefas_debug {
                                let dir = format!("artifacts/tefas_parity/{}", symbol);
                                let _ = std::fs::create_dir_all(&dir);
                                let fname = format!("{}/lib_error.txt", dir);
                                let _ = std::fs::write(&fname, format!("{}", e));
                            }
                            out.insert(canonical.to_string(), json!([]));
                            continue;
                        }
                    };
                    write_parity(symbol, "lib_raw", &v);
                    if let Some(arr) = v.get("fonFiyatBilgiGetir").and_then(|o| o.get("resultList")).and_then(|r| r.as_array()) {
                        if let Some(latest) = arr.last() {
                            if let Some(obj) = latest.as_object() {
                                let tarih = obj.get("tarih").and_then(|v| v.as_str()).unwrap_or_default();
                                let ts_ms = NaiveDate::parse_from_str(tarih, "%Y-%m-%d").ok().map(|d| d.and_hms_opt(0, 0, 0).unwrap()).map(|ndt| Utc.from_utc_datetime(&ndt).timestamp_millis()).unwrap_or(0_i64);
                                let price = obj.get("fiyat").cloned().or_else(|| obj.get("price").cloned()).unwrap_or(Value::Null);
                                let mut rec = serde_json::Map::new();
                                rec.insert("timestamp_ms".to_string(), Value::from(ts_ms));
                                rec.insert("last".to_string(), price.clone());
                                rec.insert("price".to_string(), price);
                                out.insert(canonical.to_string(), Value::Array(vec![Value::Object(rec)]));
                            } else {
                                out.insert(canonical.to_string(), json!([]));
                            }
                        } else {
                            out.insert(canonical.to_string(), json!([]));
                        }
                    } else {
                        out.insert(canonical.to_string(), json!([]));
                    }
                }

                // For other datasets we currently don't support them via TEFAS library
                _ => {
                    out.insert(canonical.to_string(), json!([]));
                }
            }
        }

        Ok(out)
    }
}
