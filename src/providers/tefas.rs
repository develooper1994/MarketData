use crate::hub::RawSourceAdapter;
use crate::providers::errors::ProviderError;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::Command;
use chrono::{NaiveDate, Utc, TimeZone};
use tefas::{AppConfig, TefasClient, QueryBatchRequest, HttpBackend, DEFAULT_USER_AGENT};
use tefas::QueryOperationName;
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

// Shared runtime to avoid creating a new Runtime per call which is expensive
static SHARED_RT: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("failed to create tokio runtime"));

pub struct TefasAdapter {
    client: reqwest::blocking::Client,
}

impl Default for TefasAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
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
        let base = std::env::var("TEFAS_BASE_URL").unwrap_or_else(|_| "https://www.tefas.gov.tr".to_string());
        let base = base.trim_end_matches('/');

        // Helper to try running the external tefas-cli binary and parse JSON stdout
        fn try_run_tefas_cli(cmd: &str, args: &[&str]) -> Result<Value, ProviderError> {
            let output = Command::new(cmd)
                .args(args)
                .output()
                .map_err(|e| ProviderError::Other(format!("failed to spawn {}: {}", cmd, e)))?;

            let stdout_lossy = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr_lossy = String::from_utf8_lossy(&output.stderr).into_owned();

            if !output.status.success() {
                return Err(ProviderError::Other(format!(
                    "{} exited {}. stdout: {} stderr: {}",
                    cmd, output.status, stdout_lossy, stderr_lossy
                )));
            }

            serde_json::from_str::<Value>(&stdout_lossy).map_err(|e| {
                ProviderError::ParseError(format!(
                    "failed to parse tefas-cli output as json: {}; stdout: {}; stderr: {}",
                    e, stdout_lossy, stderr_lossy
                ))
            })
        }

        let tefas_debug = std::env::var("TEFAS_DEBUG").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);

            // Helper to persist parity artifacts when debugging is enabled
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

        for ds in datasets {
            let canonical = crate::capabilities::canonical_dataset_name(ds);
            match canonical {
                "fundamentals" | "corporate_actions" | "fund_nav" => {
                    // Prefer using an external tefas-cli tool; allow override via TEFAS_CLI_CMD
                    // Prefer the in-repo `tefas` library when available (compile-time dependency).
                    // Create a temporary tokio runtime to execute the async client and query
                    // the `fonFiyatBilgiGetir` operation with `periyod=1` set override.
                    let lib_res = || -> Result<Value, ProviderError> {
                        let mut cfg = AppConfig::with_defaults();
                        if let Ok(env_base) = std::env::var("TEFAS_BASE_URL") {
                            if !env_base.trim().is_empty() { cfg.base_url = env_base; }
                        }

                        // Explicitly prefer the CLI-like defaults for parity
                        cfg.auth.user_agent = DEFAULT_USER_AGENT.to_string();
                        cfg.backend = HttpBackend::Wreq;

                        let rt = &*SHARED_RT;
                        let client = rt.block_on(async { TefasClient::new(cfg).map_err(|e| ProviderError::Other(format!("tefas client init: {}", e))) })?;

                        // Attempt preflight to warm up session/cookies similar to CLI
                        if let Err(e) = rt.block_on(async { client.preflight().await }) {
                            // record the preflight error for diagnostics
                            return Err(ProviderError::Other(format!("tefas preflight failed: {}", e)));
                        }

                        let req = QueryBatchRequest::new(vec![QueryOperationName::new("fonFiyatBilgiGetir")], None);
                        let overrides = vec![("fonKodu".to_string(), Value::String(symbol.to_string())), ("periyod".to_string(), Value::String("1".to_string()))];
                        let v = rt.block_on(async { client.query_by_names(req, overrides, None).await.map_err(|e| ProviderError::Other(format!("tefas query error: {}", e))) })?;
                        Ok(v)
                    };

                    match lib_res() {
                        Ok(lib_json) => {
                            // Persist raw library JSON for forensic parity (even on success)
                            out.insert(format!("{}_tefas_lib_raw", canonical), lib_json.clone());
                            write_parity(symbol, "lib_raw", &lib_json);
                            // Only accept library results when the expected `resultList` exists.
                            if let Some(arr) = lib_json
                                .get("fonFiyatBilgiGetir")
                                .and_then(|o| o.get("resultList"))
                                .and_then(|r| r.as_array())
                            {
                                let mut out_arr = Vec::new();
                                for item in arr {
                                    if let Some(obj) = item.as_object() {
                                        let tarih = obj.get("tarih").and_then(|v| v.as_str()).unwrap_or_default();
                                        let ts_ms = NaiveDate::parse_from_str(tarih, "%Y-%m-%d")
                                            .ok()
                                            .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                                            .map(|ndt| Utc.from_utc_datetime(&ndt).timestamp_millis())
                                            .unwrap_or(0_i64);

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
                                continue;
                            } else {
                                    // Record raw library response for diagnostics and fall through to CLI/HTTP fallbacks
                                    out.insert(format!("{}_tefas_lib_raw", canonical), lib_json.clone());
                                    write_parity(symbol, "lib_raw", &lib_json);
                            }
                        }
                        Err(e) => {
                            if tefas_debug {
                                eprintln!("tefas library error for {}: {}", canonical, e);
                            }
                            // record the error in the output map for forensic comparison
                            out.insert(format!("{}_tefas_lib_error", canonical), Value::String(format!("{}", e)));
                            // fall through to CLI/HTTP fallbacks
                        }
                    }

                    // Build candidate CLI commands (prefer env override, then local build, then common names)
                    let mut possible_cmds: Vec<String> = Vec::new();
                    if let Ok(env_cmd) = std::env::var("TEFAS_CLI_CMD") {
                        if !env_cmd.trim().is_empty() { possible_cmds.push(env_cmd); }
                    }
                    if let Ok(cwd) = std::env::current_dir() {
                        let local = cwd.join("external_tools/tefas-cli/target/release/cli");
                        if local.exists() {
                            possible_cmds.push(local.to_string_lossy().to_string());
                        }
                    }
                    possible_cmds.extend(vec!["tefas-cli".to_string(), "cli".to_string(), "tefas".to_string()]);

                    // Candidate argument patterns (try in order). We build owned Strings
                    // so we can include dynamic `symbol` values.
                    let candidates: Vec<Vec<String>> = vec![
                        // Prefer explicit Takasbank query with periyod (price series)
                        vec!["query".to_string(), "fonFiyatBilgiGetir".to_string(), "--set".to_string(), format!("fonKodu={}", symbol), "--set".to_string(), "periyod=1".to_string(), "--format".to_string(), "json".to_string()],
                        vec!["values".to_string(), symbol.to_string(), "--json".to_string()],
                        vec!["values".to_string(), symbol.to_string()],
                        vec!["fund".to_string(), symbol.to_string(), "--json".to_string()],
                        vec!["funds".to_string(), symbol.to_string(), "--json".to_string()],
                        vec!["get".to_string(), "values".to_string(), symbol.to_string(), "--json".to_string()],
                    ];

                    let mut inserted = false;
                    for cmd in &possible_cmds {
                        for args in &candidates {
                            let arg_slices: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                            match try_run_tefas_cli(cmd, &arg_slices) {
                                Ok(json_v) => {
                                    // also save a raw copy of CLI output for diagnostics
                                    out.insert(format!("{}_tefas_cli_raw", canonical), json_v.clone());
                                    write_parity(symbol, "cli_raw", &json_v);
                                    out.insert(canonical.to_string(), json_v);
                                    inserted = true;
                                    break;
                                }
                                Err(_) => continue,
                            }
                        }
                        if inserted { break; }
                    }
                    if inserted {
                        continue;
                    }

                    // Fallback: try the HTTP endpoint as a last resort and capture traces when enabled
                    let url = format!("{}/api/values?symbol={}", base, symbol);
                    if let Ok(resp) = self.client.get(&url).send() {
                        let status_u16 = resp.status().as_u16();
                        let headers_clone = resp.headers().clone();
                        if let Ok(text_body) = resp.text() {
                            if tefas_debug {
                                let dir = format!("artifacts/tefas_traces/{}", symbol);
                                let _ = std::fs::create_dir_all(&dir);
                                let fname = format!("{}/{}_http_{}.log", dir, canonical, Utc::now().timestamp_millis());
                                let content = format!("URL: {}\nStatus: {}\n\nHEADERS:\n{:?}\n\nBODY:\n{}\n", url, status_u16, headers_clone, text_body);
                                let _ = std::fs::write(&fname, &content);
                                out.insert(format!("{}_tefas_http_trace", canonical), Value::String(fname));
                            }
                            if let Ok(json_v) = serde_json::from_str::<Value>(&text_body) {
                                out.insert(canonical.to_string(), json_v);
                                continue;
                            }
                        }
                    }

                    out.insert(canonical.to_string(), json!([]));
                }

                // Map TEFAS fund price series into `kline` (daily OHLCV) by
                // using the `fonFiyatBilgiGetir` operation and synthesising
                // OHLC from the reported `fiyat` (price). `timestamp_ms` is
                // derived from the `tarih` field.
                "kline" => {
                    // Build candidate CLI commands (prefer env override, then local build, then common names)
                    let mut possible_cmds: Vec<String> = Vec::new();
                    if let Ok(env_cmd) = std::env::var("TEFAS_CLI_CMD") {
                        if !env_cmd.trim().is_empty() { possible_cmds.push(env_cmd); }
                    }
                    if let Ok(cwd) = std::env::current_dir() {
                        let local = cwd.join("external_tools/tefas-cli/target/release/cli");
                        if local.exists() {
                            possible_cmds.push(local.to_string_lossy().to_string());
                        }
                    }
                    possible_cmds.extend(vec!["tefas-cli".to_string(), "cli".to_string(), "tefas".to_string()]);

                    let args = vec![
                        "query".to_string(),
                        "fonFiyatBilgiGetir".to_string(),
                        "--set".to_string(),
                        format!("fonKodu={}", symbol),
                        "--set".to_string(),
                        "periyod=1".to_string(),
                        "--format".to_string(),
                        "json".to_string(),
                    ];

                    let arg_slices: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                    let mut used = false;
                    for cmd in &possible_cmds {
                        if let Ok(json_v) = try_run_tefas_cli(cmd, &arg_slices) {
                            out.insert(format!("{}_tefas_cli_raw", canonical), json_v.clone());
                            write_parity(symbol, "cli_raw", &json_v);
                            if let Some(arr) = json_v
                                .get("fonFiyatBilgiGetir")
                                .and_then(|o| o.get("resultList"))
                                .and_then(|r| r.as_array())
                            {
                                let mut out_arr = Vec::new();
                                for item in arr {
                                    if let Some(obj) = item.as_object() {
                                        let tarih = obj.get("tarih").and_then(|v| v.as_str()).unwrap_or_default();
                                        let ts_ms = NaiveDate::parse_from_str(tarih, "%Y-%m-%d")
                                            .ok()
                                            .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                                            .map(|ndt| Utc.from_utc_datetime(&ndt).timestamp_millis())
                                            .unwrap_or(0_i64);

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
                                used = true;
                                break;
                            } else {
                                out.insert(canonical.to_string(), json_v);
                                used = true;
                                break;
                            }
                        }
                    }

                    if used { continue; }

                    // HTTP fallback
                    let url = format!("{}/api/values?symbol={}", base, symbol);
                    if let Ok(resp) = self.client.get(&url).send() {
                        let status_u16 = resp.status().as_u16();
                        let headers_clone = resp.headers().clone();
                        if let Ok(text_body) = resp.text() {
                            if tefas_debug {
                                let dir = format!("artifacts/tefas_traces/{}", symbol);
                                let _ = std::fs::create_dir_all(&dir);
                                let fname = format!("{}/{}_http_{}.log", dir, canonical, Utc::now().timestamp_millis());
                                let content = format!("URL: {}\nStatus: {}\n\nHEADERS:\n{:?}\n\nBODY:\n{}\n", url, status_u16, headers_clone, text_body);
                                let _ = std::fs::write(&fname, &content);
                                out.insert(format!("{}_tefas_http_trace", canonical), Value::String(fname));
                            }
                            if let Ok(json_v) = serde_json::from_str::<Value>(&text_body) {
                                if let Some(arr) = json_v
                                    .get("fonFiyatBilgiGetir")
                                    .and_then(|o| o.get("resultList"))
                                    .and_then(|r| r.as_array())
                                {
                                    let mut out_arr = Vec::new();
                                    for item in arr {
                                        if let Some(obj) = item.as_object() {
                                            let tarih = obj.get("tarih").and_then(|v| v.as_str()).unwrap_or_default();
                                            let ts_ms = NaiveDate::parse_from_str(tarih, "%Y-%m-%d")
                                                .ok()
                                                .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                                                .map(|ndt| Utc.from_utc_datetime(&ndt).timestamp_millis())
                                                .unwrap_or(0_i64);
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
                                    continue;
                                }
                                out.insert(canonical.to_string(), json_v);
                                continue;
                            }
                        }
                    }

                    out.insert(canonical.to_string(), json!([]));
                }

                // Provide a simple `tick` view derived from the latest reported
                // fund price (most recent `resultList` entry). This synthesises a
                // best-effort snapshot with `last`/`price` fields.
                "tick" => {
                    // Prefer the in-repo `tefas` library first for a single latest-tick snapshot.
                    let lib_res = || -> Result<Value, ProviderError> {
                        let mut cfg = AppConfig::with_defaults();
                        if let Ok(env_base) = std::env::var("TEFAS_BASE_URL") {
                            if !env_base.trim().is_empty() { cfg.base_url = env_base; }
                        }

                        // Enforce CLI-like network settings for parity
                        cfg.auth.user_agent = DEFAULT_USER_AGENT.to_string();
                        cfg.backend = HttpBackend::Wreq;

                        let rt = &*SHARED_RT;
                        let client = rt.block_on(async { TefasClient::new(cfg).map_err(|e| ProviderError::Other(format!("tefas client init: {}", e))) })?;

                        // Attempt preflight to warm up session/cookies similar to CLI
                        if let Err(e) = rt.block_on(async { client.preflight().await }) {
                            return Err(ProviderError::Other(format!("tefas preflight failed: {}", e)));
                        }

                        let req = QueryBatchRequest::new(vec![QueryOperationName::new("fonFiyatBilgiGetir")], None);
                        let overrides = vec![("fonKodu".to_string(), Value::String(symbol.to_string())), ("periyod".to_string(), Value::String("1".to_string()))];
                        let v = rt.block_on(async { client.query_by_names(req, overrides, None).await.map_err(|e| ProviderError::Other(format!("tefas query error: {}", e))) })?;
                        Ok(v)
                    };

                    match lib_res() {
                        Ok(lib_json) => {
                            // Persist raw library JSON for forensic parity (even on success)
                            out.insert(format!("{}_tefas_lib_raw", canonical), lib_json.clone());
                            write_parity(symbol, "lib_raw", &lib_json);
                            if let Some(arr) = lib_json
                                .get("fonFiyatBilgiGetir")
                                .and_then(|o| o.get("resultList"))
                                .and_then(|r| r.as_array())
                            {
                                if let Some(latest) = arr.last() {
                                    if let Some(obj) = latest.as_object() {
                                        let tarih = obj.get("tarih").and_then(|v| v.as_str()).unwrap_or_default();
                                        let ts_ms = NaiveDate::parse_from_str(tarih, "%Y-%m-%d")
                                            .ok()
                                            .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                                            .map(|ndt| Utc.from_utc_datetime(&ndt).timestamp_millis())
                                            .unwrap_or(0_i64);
                                        let price = obj.get("fiyat").cloned().or_else(|| obj.get("price").cloned()).unwrap_or(Value::Null);
                                        let mut rec = serde_json::Map::new();
                                        rec.insert("timestamp_ms".to_string(), Value::from(ts_ms));
                                        rec.insert("last".to_string(), price.clone());
                                        rec.insert("price".to_string(), price);
                                        out.insert(canonical.to_string(), Value::Array(vec![Value::Object(rec)]));
                                        continue;
                                    }
                                }
                            }
                            // record raw library response and fall through to CLI/HTTP
                            out.insert(format!("{}_tefas_lib_raw", canonical), lib_json.clone());
                            write_parity(symbol, "lib_raw", &lib_json);
                        }
                        Err(e) => {
                            if tefas_debug {
                                eprintln!("tefas library error for tick {}: {}", symbol, e);
                            }
                            // fall through to CLI/HTTP fallbacks
                        }
                    }
                            // Try CLI candidates (env, local build, common names)
                    let mut possible_cmds: Vec<String> = Vec::new();
                    if let Ok(env_cmd) = std::env::var("TEFAS_CLI_CMD") {
                        if !env_cmd.trim().is_empty() { possible_cmds.push(env_cmd); }
                    }
                    if let Ok(cwd) = std::env::current_dir() {
                        let local = cwd.join("external_tools/tefas-cli/target/release/cli");
                        if local.exists() {
                            possible_cmds.push(local.to_string_lossy().to_string());
                        }
                    }
                    possible_cmds.extend(vec!["tefas-cli".to_string(), "cli".to_string(), "tefas".to_string()]);

                    let args = vec![
                        "query".to_string(),
                        "fonFiyatBilgiGetir".to_string(),
                        "--set".to_string(),
                        format!("fonKodu={}", symbol),
                        "--set".to_string(),
                        "periyod=1".to_string(),
                        "--format".to_string(),
                        "json".to_string(),
                    ];

                    let arg_slices: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                    let mut used = false;
                    for cmd in &possible_cmds {
                        if let Ok(json_v) = try_run_tefas_cli(cmd, &arg_slices) {
                            out.insert(format!("{}_tefas_cli_raw", canonical), json_v.clone());
                            write_parity(symbol, "cli_raw", &json_v);
                            if let Some(arr) = json_v
                                .get("fonFiyatBilgiGetir")
                                .and_then(|o| o.get("resultList"))
                                .and_then(|r| r.as_array())
                            {
                                if let Some(latest) = arr.last() {
                                    if let Some(obj) = latest.as_object() {
                                        let tarih = obj.get("tarih").and_then(|v| v.as_str()).unwrap_or_default();
                                        let ts_ms = NaiveDate::parse_from_str(tarih, "%Y-%m-%d")
                                            .ok()
                                            .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                                            .map(|ndt| Utc.from_utc_datetime(&ndt).timestamp_millis())
                                            .unwrap_or(0_i64);
                                        let price = obj.get("fiyat").cloned().or_else(|| obj.get("price").cloned()).unwrap_or(Value::Null);
                                        let mut rec = serde_json::Map::new();
                                        rec.insert("timestamp_ms".to_string(), Value::from(ts_ms));
                                        rec.insert("last".to_string(), price.clone());
                                        rec.insert("price".to_string(), price);
                                        out.insert(canonical.to_string(), Value::Array(vec![Value::Object(rec)]));
                                        used = true;
                                        break;
                                    }
                                }
                            }
                            out.insert(canonical.to_string(), json_v);
                            used = true;
                            break;
                        }
                    }
                    if used { continue; }

                    // HTTP fallback
                    let url = format!("{}/api/values?symbol={}", base, symbol);
                    if let Ok(resp) = self.client.get(&url).send() {
                        let status_u16 = resp.status().as_u16();
                        let headers_clone = resp.headers().clone();
                        if let Ok(text_body) = resp.text() {
                            if tefas_debug {
                                let dir = format!("artifacts/tefas_traces/{}", symbol);
                                let _ = std::fs::create_dir_all(&dir);
                                let fname = format!("{}/{}_http_{}.log", dir, canonical, Utc::now().timestamp_millis());
                                let content = format!("URL: {}\nStatus: {}\n\nHEADERS:\n{:?}\n\nBODY:\n{}\n", url, status_u16, headers_clone, text_body);
                                let _ = std::fs::write(&fname, &content);
                                out.insert(format!("{}_tefas_http_trace", canonical), Value::String(fname));
                            }
                            if let Ok(json_v) = serde_json::from_str::<Value>(&text_body) {
                                if let Some(arr) = json_v
                                    .get("fonFiyatBilgiGetir")
                                    .and_then(|o| o.get("resultList"))
                                    .and_then(|r| r.as_array())
                                {
                                    if let Some(latest) = arr.last() {
                                        if let Some(obj) = latest.as_object() {
                                            let tarih = obj.get("tarih").and_then(|v| v.as_str()).unwrap_or_default();
                                            let ts_ms = NaiveDate::parse_from_str(tarih, "%Y-%m-%d")
                                                .ok()
                                                .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                                                .map(|ndt| Utc.from_utc_datetime(&ndt).timestamp_millis())
                                                .unwrap_or(0_i64);
                                            let price = obj.get("fiyat").cloned().or_else(|| obj.get("price").cloned()).unwrap_or(Value::Null);
                                            let mut rec = serde_json::Map::new();
                                            rec.insert("timestamp_ms".to_string(), Value::from(ts_ms));
                                            rec.insert("last".to_string(), price.clone());
                                            rec.insert("price".to_string(), price);
                                            out.insert(canonical.to_string(), Value::Array(vec![Value::Object(rec)]));
                                            continue;
                                        }
                                    }
                                }
                                out.insert(canonical.to_string(), json_v);
                                continue;
                            }
                        }
                    }

                    out.insert(canonical.to_string(), json!([]));
                }

                _ => {}
            }
        }

        Ok(out)
    }
}
