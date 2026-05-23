use market_data::{
    DataHub, InMemoryStorage, LocalArtifactStorage, ManifestProvenanceTracker,
    SourceAdapterRegistry, all_capabilities, asset_status_for_source, available_datasets,
    best_sources_for, capability_map, dataset_source_matrix, dataset_status_for_source,
    dataset_summary, recommend_sources_for_use_case, source_summary, sources_for,
    supported_use_cases,
};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use reqwest::blocking::Client;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::env;
use std::io::{self, IsTerminal, Read};
use std::time::Duration;
use std::process::Command;

/// Increment this any time the bridge JSON contract changes incompatibly.
const BRIDGE_CONTRACT_VERSION: &str = "1";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let json_mode = if let Some(index) = args.iter().position(|item| item == "--json") {
        args.remove(index);
        true
    } else {
        false
    };

    let command = args.first().map(String::as_str);
    let command_args = if args.len() > 1 {
        args[1..].to_vec()
    } else {
        Vec::new()
    };

    if json_mode {
        let command = command.ok_or("json mode requires an operation")?;
        return execute_json_operation(command);
    }

    if command.is_none() && try_handle_stdin_request_mode()? {
        return Ok(());
    }

    execute_command(canonical_command(command), command, command_args, None)
}

fn execute_json_operation(operation: &str) -> Result<(), Box<dyn std::error::Error>> {
    let payload = read_json_payload()?;
    let caps = capability_map();

    match operation {
        "sources" => print_json(&json!(all_capabilities().into_iter().map(|c| c.source).collect::<Vec<_>>()), false)?,
        "capabilities" => print_json(&serde_json::to_value(all_capabilities())?, false)?,
        "capability" => {
            let source = payload_string(&payload, "source").unwrap_or_default();
            let value = caps
                .get(source.as_str())
                .map(serde_json::to_value)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            print_json(&value, false)?;
        }
        "coverage_table" => {
            let mut rows: Vec<Value> = Vec::new();
            for cap in caps.values() {
                for dataset in &cap.datasets {
                    rows.push(json!({
                        "source": cap.source,
                        "dataset": dataset,
                        "status": dataset_status_for_source(&caps, &cap.source, dataset),
                    }));
                }
            }
            print_json(&json!(rows), false)?;
        }
        "dataset_status" => {
            let source = payload_string(&payload, "source").unwrap_or_default();
            let dataset = payload_string(&payload, "dataset").unwrap_or_default();
            print_json(&json!(dataset_status_for_source(&caps, &source, &dataset)), false)?;
        }
        "asset_status" => {
            let source = payload_string(&payload, "source").unwrap_or_default();
            let asset_class = payload_string(&payload, "asset_class").unwrap_or_default();
            print_json(&json!(asset_status_for_source(&caps, &source, &asset_class)), false)?;
        }
        "supports" => {
            let source = payload_string(&payload, "source").unwrap_or_default();
            let dataset = payload_string(&payload, "dataset").unwrap_or_default();
            let require_live = payload_bool(&payload, "require_live").unwrap_or(false);
            let supported = sources_for(&caps, Some(&dataset), None, require_live)
                .into_iter()
                .any(|item| item == source);
            print_json(&json!(supported), false)?;
        }
        "sources_for" => {
            let dataset = payload_string(&payload, "dataset");
            let asset_class = payload_string(&payload, "asset_class");
            let require_live = payload_bool(&payload, "require_live").unwrap_or(false);
            let result = sources_for(&caps, dataset.as_deref(), asset_class.as_deref(), require_live);
            print_json(&json!(result), false)?;
        }
        "available_datasets" => {
            let source = payload_string(&payload, "source").unwrap_or_default();
            let implemented_only = payload_bool(&payload, "implemented_only").unwrap_or(false);
            let result = available_datasets(&caps, &source, implemented_only);
            print_json(&json!(result), false)?;
        }
        "compare_sources" => {
            let sources = payload
                .get("sources")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(Value::as_str)
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let datasets = payload
                .get("datasets")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(Value::as_str)
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut rows: Vec<Value> = Vec::new();
            for source in sources {
                if let Some(cap) = caps.get(&source) {
                    let candidate = if datasets.is_empty() {
                        cap.datasets.clone()
                    } else {
                        datasets.clone()
                    };
                    let implemented = candidate
                        .iter()
                        .filter(|dataset| {
                            cap.implemented_datasets
                                .iter()
                                .any(|item| item == *dataset)
                        })
                        .count();
                    rows.push(json!({
                        "source": cap.source,
                        "quality_level": cap.quality_level,
                        "implementation_status": cap.implementation_status,
                        "supports_realtime": if cap.supports_realtime { "true" } else { "false" },
                        "requires_api_key": if cap.requires_api_key { "true" } else { "false" },
                        "implemented_dataset_count": implemented.to_string(),
                    }));
                }
            }
            print_json(&json!(rows), false)?;
        }
        "source_summary" | "explain_source" => {
            let source = payload_string(&payload, "source").unwrap_or_default();
            let value = caps
                .get(source.as_str())
                .map(serde_json::to_value)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            print_json(&value, false)?;
        }
        "best_sources_for" => {
            let dataset = payload_string(&payload, "dataset").unwrap_or_else(|| "kline".to_string());
            let asset_class = payload_string(&payload, "asset_class");
            let prefer_live = payload_bool(&payload, "prefer_live").unwrap_or(true);
            let allow_api_key = payload_bool(&payload, "allow_api_key").unwrap_or(true);
            let include_metadata_only = payload_bool(&payload, "include_metadata_only").unwrap_or(false);
            let limit = payload
                .get("limit")
                .and_then(Value::as_u64)
                .map(|value| value as usize);
            let rows = best_sources_for(
                &caps,
                &dataset,
                asset_class.as_deref(),
                prefer_live,
                allow_api_key,
                include_metadata_only,
                limit,
            );
            print_json(&json!(rows), false)?;
        }
        "explain_dataset" => {
            let dataset = payload_string(&payload, "dataset").unwrap_or_else(|| "kline".to_string());
            let mut summary = dataset_summary(&caps, &dataset);
            let best_no_api = best_sources_for(
                &caps,
                &dataset,
                None,
                true,
                false,
                true,
                Some(5),
            );
            let best_with_api = best_sources_for(
                &caps,
                &dataset,
                None,
                true,
                true,
                true,
                Some(5),
            );
            summary.insert("best_sources_no_api_key".to_string(), json!(best_no_api));
            summary.insert("best_sources_with_api_key".to_string(), json!(best_with_api));
            print_json(&json!(summary), false)?;
        }
        "recommend_sources" => {
            let use_case = payload_string(&payload, "use_case").unwrap_or_default();
            let allow_api_key = payload_bool(&payload, "allow_api_key").unwrap_or(true);
            let prefer_live = payload_bool(&payload, "prefer_live").unwrap_or(true);
            let limit = payload
                .get("limit")
                .and_then(Value::as_u64)
                .map(|value| value as usize);
            let rows = recommend_sources_for_use_case(&caps, &use_case, allow_api_key, prefer_live, limit);
            print_json(&json!(rows), false)?;
        }
        "supported_use_cases" => {
            print_json(&json!(supported_use_cases()), false)?;
        }
        "dataset_sources_matrix" => {
            let selected = payload
                .get("datasets")
                .and_then(Value::as_array)
                .map(|rows| rows.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                .unwrap_or_default();
            let matrix = dataset_source_matrix(&caps);
            let mut rows = Vec::new();
            if selected.is_empty() {
                for item in matrix.values() {
                    rows.push(item.clone());
                }
            } else {
                for dataset in selected {
                    if let Some(item) = matrix.get(dataset) {
                        rows.push(item.clone());
                    }
                }
            }
            print_json(&json!(rows), false)?;
        }
        "asset_sources_matrix" => {
            let selected = payload
                .get("asset_classes")
                .and_then(Value::as_array)
                .map(|rows| rows.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                .unwrap_or_default();
            let asset_classes = if selected.is_empty() {
                let mut unique = std::collections::BTreeSet::new();
                for cap in caps.values() {
                    for asset_class in &cap.asset_classes {
                        unique.insert(asset_class.clone());
                    }
                }
                unique.into_iter().collect::<Vec<_>>()
            } else {
                selected.iter().map(|item| (*item).to_string()).collect::<Vec<_>>()
            };
            let mut rows = Vec::new();
            for asset_class in asset_classes {
                rows.push(json!({
                    "asset_class": asset_class,
                    "sources": sources_for(&caps, None, Some(&asset_class), false).join(","),
                    "live_sources": sources_for(&caps, None, Some(&asset_class), true).join(","),
                }));
            }
            print_json(&json!(rows), false)?;
        }
        "discover_assets" => {
            let source = payload_string(&payload, "source").unwrap_or_default();
            let limit = payload
                .get("limit")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(10);
            let rows = discover_assets_live(&source, limit);
            print_json(&json!(rows), false)?;
        }
        "ingest" => {
            let options = ingest_options_from_json_payload(&payload)?;
            let (result, meta) = run_ingest_with_live_fetch(options, payload.get("fetch_options"))?;
            let mut value = serde_json::to_value(result)?;
            if let Value::Object(ref mut obj) = value {
                for (k, v) in meta { obj.insert(k, v); }
            }
            print_json(&value, false)?;
        }
        "load_market_data" => {
            let options = load_market_data_options_from_json_payload(&payload)?;
            let (result, _meta) = run_ingest_with_live_fetch(options, payload.get("fetch_options"))?;
            let dataset = result
                .requested_datasets
                .first()
                .cloned()
                .unwrap_or_else(|| "kline".to_string());
            let rows: Vec<Value> = result
                .records
                .iter()
                .filter(|record| {
                    record
                        .metadata
                        .get("dataset")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value == dataset)
                })
                .map(|record| serde_json::to_value(&record.payload).unwrap_or(Value::Object(Map::new())))
                .collect();
            print_json(&json!({ "rows": rows, "source_issues": result.source_issues }), false)?;
        }
        "doctor" | "status" => {
            execute_command(Some("doctor"), Some("doctor"), Vec::new(), None)?;
        }
        other => {
            return Err(format!("unknown json operation: {other}").into());
        }
    }

    Ok(())
}

fn read_json_payload() -> Result<Map<String, Value>, Box<dyn std::error::Error>> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    let value: Value = serde_json::from_str(&raw)?;
    Ok(value.as_object().cloned().unwrap_or_default())
}

fn payload_string(payload: &Map<String, Value>, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(ToString::to_string)
}

fn payload_bool(payload: &Map<String, Value>, key: &str) -> Option<bool> {
    payload.get(key).and_then(Value::as_bool)
}

fn execute_command(
    canonical: Option<&str>,
    command: Option<&str>,
    command_args: Vec<String>,
    ingest_input_override: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    match canonical {
        Some("help") => {
            println!("{}", help_text());
        }
        Some("doctor") => print_json(
            &json!({
                "status": "ok",
                "binary": "market_data_bridge",
                "crate": env!("CARGO_PKG_NAME"),
                "version": env!("CARGO_PKG_VERSION"),
                "contract_version": BRIDGE_CONTRACT_VERSION,
                "transport": "stdin_json",
                "supported_datasets": [
                    "kline", "tick", "trade", "orderbook", "funding",
                    "macro", "news", "fundamentals", "corporate_actions"
                ],
                "source_count": all_capabilities().len(),
            }),
            true,
        )?,
        Some("capabilities") => {
            let caps = all_capabilities();
            let value = serde_json::to_value(caps)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Some("assert-contract") => assert_contract(command_args)?,
        Some("sources") => {
            let caps = all_capabilities();
            let names: Vec<String> = caps.into_iter().map(|c| c.source).collect();
            println!("{}", serde_json::to_string_pretty(&json!(names))?);
        }
        Some("query-sources-for") => query_sources_for(command_args)?,
        Some("query-best-sources") => query_best_sources(command_args)?,
        Some("query-source-summary") => query_source_summary(command_args)?,
        Some("query-dataset-summary") => query_dataset_summary(command_args)?,
        Some("query-dataset-matrix") => query_dataset_matrix()?,
        Some("recommend-sources") => recommend_sources(command_args)?,
        Some("live-fetch") => live_fetch_command(command_args)?,
        Some("consume-streams") => consume_streams_command(command_args)?,
        Some("stream-start") => stream_start_command(command_args)?,
        Some("stream-stop") => stream_stop_command(command_args)?,
        Some("smoke") => smoke_command(command_args)?,
        Some("supported-use-cases") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!(supported_use_cases()))?
            );
        }
        Some("ingest") => ingest(parse_ingest_options(command_args)?, ingest_input_override)?,
        None => {
            println!("{}", help_text());
        }
        Some(_) => {
            let command = command.unwrap_or_default();
            return Err(format!(
                "unknown command: {command}\n\nRun `market_data_bridge help` for usage.\n\n{}",
                help_text()
            )
            .into());
        }
    }

    Ok(())
}

fn try_handle_stdin_request_mode() -> Result<bool, Box<dyn std::error::Error>> {
    if io::stdin().is_terminal() {
        return Ok(false);
    }

    let mut request = String::new();
    io::stdin().read_to_string(&mut request)?;
    if request.trim().is_empty() {
        return Ok(false);
    }

    let value: Value = serde_json::from_str(&request)
        .map_err(|error| format!("failed to parse stdin request json: {error}"))?;
    let object = value
        .as_object()
        .ok_or("stdin request must be a JSON object")?;
    let command = object
        .get("command")
        .or_else(|| object.get("cmd"))
        .or_else(|| object.get("method"))
        .or_else(|| object.get("action"))
        .and_then(Value::as_str)
        .ok_or("stdin request must include command")?;

    let args = request_args(command, object)?;
    let ingest_input_override = if command == "ingest" {
        ingest_request_payload(object)?
    } else {
        None
    };

    execute_command(
        canonical_command(Some(command)),
        Some(command),
        args,
        ingest_input_override,
    )?;
    Ok(true)
}

fn request_args(
    command: &str,
    request: &serde_json::Map<String, Value>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if let Some(args) = request.get("args")
        && let Some(array) = args.as_array()
    {
        return array
            .iter()
            .map(request_value_to_arg)
            .collect::<Result<Vec<_>, _>>();
    }

    let options = request
        .get("args")
        .and_then(Value::as_object)
        .or_else(|| request.get("options").and_then(Value::as_object))
        .unwrap_or(request);

    let args = match command {
        "assert-contract" | "assert" => option_args(options, &[("expected", "--expected")], &[])?,
        "query-sources-for" | "qsf" => option_args(
            options,
            &[("dataset", "--dataset"), ("asset_class", "--asset-class")],
            &[("require_live", "--require-live")],
        )?,
        "query-best-sources" | "qbs" => option_args(
            options,
            &[
                ("dataset", "--dataset"),
                ("asset_class", "--asset-class"),
                ("limit", "--limit"),
            ],
            &[
                ("disallow_api_key", "--disallow-api-key"),
                ("no_prefer_live", "--no-prefer-live"),
                ("include_metadata_only", "--include-metadata-only"),
            ],
        )?,
        "query-source-summary" | "qss" => option_args(options, &[("source", "--source")], &[])?,
        "query-dataset-summary" | "qds" => option_args(options, &[("dataset", "--dataset")], &[])?,
        "recommend-sources" | "rs" => option_args(
            options,
            &[("use_case", "--use-case"), ("limit", "--limit")],
            &[
                ("disallow_api_key", "--disallow-api-key"),
                ("no_prefer_live", "--no-prefer-live"),
            ],
        )?,
        "ingest" | "ing" => ingest_option_args(options)?,
        _ => Vec::new(),
    };
    Ok(args)
}

fn request_value_to_arg(value: &Value) -> Result<String, Box<dyn std::error::Error>> {
    match value {
        Value::String(v) => Ok(v.clone()),
        Value::Number(v) => Ok(v.to_string()),
        Value::Bool(v) => Ok(v.to_string()),
        _ => Err("request args array values must be string/number/bool".into()),
    }
}

fn option_args(
    options: &serde_json::Map<String, Value>,
    value_options: &[(&str, &str)],
    bool_flags: &[(&str, &str)],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut args = Vec::new();
    for (key, flag) in value_options {
        if let Some(value) = request_option(options, key) {
            args.push((*flag).to_string());
            args.push(request_value_to_arg(value)?);
        }
    }
    for (key, flag) in bool_flags {
        if request_option(options, key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            args.push((*flag).to_string());
        }
    }
    Ok(args)
}

fn ingest_option_args(
    options: &serde_json::Map<String, Value>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut args = option_args(
        options,
        &[
            ("source", "--source"),
            ("symbol", "--symbol"),
            ("asset_type", "--asset-type"),
            ("timeframe", "--timeframe"),
            ("limit", "--limit"),
            ("record_root", "--record-root"),
            ("manifest_root", "--manifest-root"),
            ("duckdb", "--duckdb"),
        ],
        &[("store", "--store")],
    )?;

    if let Some(datasets) = request_option(options, "datasets") {
        let datasets_arg = match datasets {
            Value::String(v) => v.clone(),
            Value::Array(values) => values
                .iter()
                .map(request_value_to_arg)
                .collect::<Result<Vec<_>, _>>()?
                .join(","),
            _ => return Err("datasets must be string or array".into()),
        };
        args.push("--datasets".to_string());
        args.push(datasets_arg);
    }

    if let Some(dataset) = request_option(options, "dataset") {
        match dataset {
            Value::String(v) => {
                args.push("--dataset".to_string());
                args.push(v.clone());
            }
            Value::Array(values) => {
                for value in values {
                    args.push("--dataset".to_string());
                    args.push(request_value_to_arg(value)?);
                }
            }
            _ => return Err("dataset must be string or array".into()),
        }
    }

    Ok(args)
}

fn ingest_request_payload(
    request: &serde_json::Map<String, Value>,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if let Some(value) = request_option(request, "raw_datasets")
        .or_else(|| request.get("raw"))
        .or_else(|| request.get("payload"))
    {
        return Ok(Some(serde_json::to_string(value)?));
    }

    if let Some(stdin) = request.get("stdin") {
        return match stdin {
            Value::String(v) => Ok(Some(v.clone())),
            _ => Ok(Some(serde_json::to_string(stdin)?)),
        };
    }

    Ok(None)
}

fn request_option<'a>(options: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a Value> {
    let kebab = key.replace('_', "-");
    let snake = key.replace('-', "_");
    options
        .get(key)
        .or_else(|| options.get(kebab.as_str()))
        .or_else(|| options.get(snake.as_str()))
}

fn canonical_command(command: Option<&str>) -> Option<&'static str> {
    match command {
        None => None,
        Some("help") | Some("--help") | Some("-h") => Some("help"),
        Some("doctor") | Some("status") => Some("doctor"),
        Some("assert-contract") | Some("assert") => Some("assert-contract"),
        Some("capabilities") | Some("caps") => Some("capabilities"),
        Some("sources") | Some("ls") => Some("sources"),
        Some("query-sources-for") | Some("qsf") => Some("query-sources-for"),
        Some("query-best-sources") | Some("qbs") => Some("query-best-sources"),
        Some("query-source-summary") | Some("qss") => Some("query-source-summary"),
        Some("query-dataset-summary") | Some("qds") => Some("query-dataset-summary"),
        Some("query-dataset-matrix") | Some("qdm") => Some("query-dataset-matrix"),
        Some("recommend-sources") | Some("rs") => Some("recommend-sources"),
        Some("live-fetch") | Some("lf") => Some("live-fetch"),
        Some("consume-streams") | Some("cs") => Some("consume-streams"),
        Some("stream-start") | Some("stream_start") | Some("ss") => Some("stream-start"),
        Some("stream-stop") | Some("stream_stop") => Some("stream-stop"),
        Some("smoke") | Some("sm") => Some("smoke"),
        Some("supported-use-cases") | Some("suc") => Some("supported-use-cases"),
        Some("supported_use_cases") => Some("supported-use-cases"),
        Some("ingest") | Some("ing") => Some("ingest"),
        Some("sources_for") => Some("query-sources-for"),
        Some("best_sources_for") => Some("query-best-sources"),
        Some("explain_source") | Some("source_summary") => Some("query-source-summary"),
        Some("explain_dataset") => Some("query-dataset-summary"),
        Some("recommend_sources") => Some("recommend-sources"),
        Some(_) => Some("unknown"),
    }
}

fn help_text() -> &'static str {
    r#"market_data_bridge — MarketData integration CLI

USAGE
  market_data_bridge <command> [options]
  market_data_bridge help
  market_data_bridge --help

COMMANDS
  doctor (status)                Health + contract info for automation and startup checks
  assert-contract (assert)       Fail fast if the expected contract version is not matched
  capabilities (caps)            Full provider capability metadata (all sources)
  sources (ls)                   Short source-name list for quick discovery
  query-sources-for (qsf)        Filter sources by dataset/asset class/live support
  query-best-sources (qbs)       Ranked recommendations for a dataset/use-case profile
  query-source-summary (qss)     Explain capabilities + support status for one source
  query-dataset-summary (qds)    Explain source coverage summary for one dataset
  query-dataset-matrix (qdm)     Machine-readable dataset → source coverage matrix
  supported-use-cases (suc)      List built-in recommendation flows
  recommend-sources (rs)         Recommend sources by use-case
        live-fetch (lf)                Real online fetch with normal command flags (no stdin JSON)
    consume-streams (cs)           Consume file-backed streams from artifacts/streams and ingest
    stream-start (ss)              Start a background streaming adapter for a symbol
    stream-stop                     Stop a background streaming adapter for a symbol
    smoke (sm)                     Run a live smoke-check across representative sources
    ingest (ing)                   Normalize + quality-check + storage + provenance

COMMON FLOWS
  1) Verify bridge compatibility
     market_data_bridge doctor
     market_data_bridge assert-contract --expected 1

  2) Discover source coverage
     market_data_bridge sources
     market_data_bridge capabilities
     market_data_bridge query-sources-for --dataset kline --asset-class crypto_spot

  3) Query/recommend the best source
     market_data_bridge query-best-sources --dataset kline --asset-class crypto_spot --limit 5
     market_data_bridge query-source-summary --source binance_futures
     market_data_bridge query-dataset-summary --dataset kline
     market_data_bridge query-dataset-matrix
     market_data_bridge supported-use-cases
     market_data_bridge recommend-sources --use-case crypto_backtest --limit 5

  4) Ingest raw payload through the full pipeline
     printf '{"kline":[[1716200000000,"10","11","9","10.5","42"]]}' | \
       market_data_bridge ingest --source offline --symbol BTCUSDT --datasets kline --asset-type crypto_spot

ONLINE DATA (REAL FETCH)
  Single-command live fetch:

  1) Fetch live rows directly (no printf, no stdin JSON)
      market_data_bridge live-fetch --source binance_futures --symbol BTCUSDT --dataset tick --limit 5

  2) Fetch multiple datasets in one call
      market_data_bridge live-fetch --source binance_futures --symbol BTCUSDT --datasets tick,funding --asset-type crypto_perp --limit 5

  3) Find live-capable sources for your dataset/asset class
      market_data_bridge query-sources-for --dataset tick --asset-class crypto_spot --require-live

  4) Advanced/compat mode: JSON operation path
      printf '{"source":"binance_futures","symbol":"BTCUSDT","dataset":"tick","limit":5}' | \
         market_data_bridge --json load_market_data

  5) Advanced/compat: JSON ingest operation
      printf '{"source":"binance_futures","symbol":"BTCUSDT","datasets":["tick","funding"],"asset_type":"crypto_perp","limit":5}' | \
         market_data_bridge --json ingest

  6) Optional: list example symbols discovered from a source
      printf '{"source":"binance_futures","limit":5}' | market_data_bridge --json discover_assets

  7) Error semantics in source_issues
      api_key_required:<ENV_NAME>
      rate_limited:<source>
      network_error:<source>:<detail>
      unsupported_dataset:<dataset>

MORE EXAMPLES
  market_data_bridge doctor
  market_data_bridge query-sources-for --dataset kline --asset-class crypto_spot
  market_data_bridge query-best-sources --dataset fundamentals --include-metadata-only
  market_data_bridge query-dataset-matrix
  market_data_bridge recommend-sources --use-case crypto_backtest --limit 5
  market_data_bridge live-fetch --source binance_futures --symbol BTCUSDT --dataset tick --limit 5
  printf '{"kline":[[1716200000000,"10","11","9","10.5","42"]]}' | \
    market_data_bridge ingest --source offline --symbol BTCUSDT --datasets kline --asset-type crypto_spot
"#
}

fn live_fetch_command(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_ingest_options(args)?;
    let source = options.source.clone();
    let symbol = options.symbol.clone();
    let requested_datasets = options.datasets.clone();

    let (result, meta) = run_ingest_with_live_fetch(options, None)?;
    let mut rows_by_dataset = Map::new();
    for dataset in &result.requested_datasets {
        let rows: Vec<Value> = result
            .records
            .iter()
            .filter(|record| {
                record
                    .metadata
                    .get("dataset")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value == dataset)
            })
            .map(|record| {
                serde_json::to_value(&record.payload).unwrap_or(Value::Object(Map::new()))
            })
            .collect();
        rows_by_dataset.insert(dataset.clone(), Value::Array(rows));
    }

    let primary_dataset = result
        .requested_datasets
        .first()
        .cloned()
        .or_else(|| requested_datasets.first().cloned())
        .unwrap_or_else(|| "kline".to_string());
    let rows = rows_by_dataset
        .get(&primary_dataset)
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));

    let mut payload = Map::new();
    // selected_source in metadata takes precedence when auto-selection was used
    let selected_source = meta
        .get("selected_source")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| source.clone());
    payload.insert("source".to_string(), json!(selected_source));
    payload.insert("symbol".to_string(), json!(symbol));
    payload.insert("dataset".to_string(), json!(primary_dataset));
    payload.insert("datasets".to_string(), json!(result.requested_datasets));
    payload.insert("rows".to_string(), rows);
    if rows_by_dataset.len() > 1 {
        payload.insert("rows_by_dataset".to_string(), Value::Object(rows_by_dataset));
    }
    payload.insert("source_issues".to_string(), json!(result.source_issues));
    payload.insert("dataset_coverage".to_string(), json!(result.dataset_coverage));
    // Include raw provider payloads for diagnostics when performing live-fetch
    payload.insert("raw_datasets".to_string(), json!(result.raw_datasets));
    // Merge selection metadata into the live-fetch payload when present
    for key in ["selection_mode", "attempted_sources", "fallback_used", "fallback_reasons", "warnings"] {
        if let Some(val) = meta.get(key) {
            payload.insert(key.to_string(), val.clone());
        }
    }

    print_json(&Value::Object(payload), false)?;
    Ok(())
}

fn assert_contract(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut expected: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--expected" => {
                i += 1;
                expected = args.get(i).cloned();
            }
            unknown => return Err(format!("unknown option: {unknown}").into()),
        }
        i += 1;
    }

    let expected = expected.ok_or("--expected is required")?;
    let actual = BRIDGE_CONTRACT_VERSION;
    if expected != actual {
        return Err(
            format!("contract version mismatch: expected {expected}, actual {actual}").into(),
        );
    }

    print_json(
        &json!({
            "status": "ok",
            "expected": expected,
            "actual": actual,
            "compatible": true
        }),
        false,
    )?;
    Ok(())
}

fn query_sources_for(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut dataset: Option<String> = None;
    let mut asset_class: Option<String> = None;
    let mut require_live = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dataset" => {
                i += 1;
                dataset = args.get(i).cloned();
            }
            "--asset-class" => {
                i += 1;
                asset_class = args.get(i).cloned();
            }
            "--require-live" => {
                require_live = true;
            }
            unknown => return Err(format!("unknown option: {unknown}").into()),
        }
        i += 1;
    }

    let caps = capability_map();
    let result = sources_for(
        &caps,
        dataset.as_deref(),
        asset_class.as_deref(),
        require_live,
    );
    println!("{}", serde_json::to_string_pretty(&json!(result))?);
    Ok(())
}

fn query_best_sources(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut dataset: Option<String> = None;
    let mut asset_class: Option<String> = None;
    let mut prefer_live = true;
    let mut allow_api_key = true;
    let mut include_metadata_only = false;
    let mut limit: Option<usize> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dataset" => {
                i += 1;
                dataset = args.get(i).cloned();
            }
            "--asset-class" => {
                i += 1;
                asset_class = args.get(i).cloned();
            }
            "--limit" => {
                i += 1;
                let parsed = args.get(i).ok_or("--limit requires a value")?;
                limit = Some(parsed.parse::<usize>()?);
            }
            "--disallow-api-key" => allow_api_key = false,
            "--no-prefer-live" => prefer_live = false,
            "--include-metadata-only" => include_metadata_only = true,
            unknown => return Err(format!("unknown option: {unknown}").into()),
        }
        i += 1;
    }

    let dataset = dataset.ok_or("--dataset is required")?;
    let caps = capability_map();
    let result = best_sources_for(
        &caps,
        &dataset,
        asset_class.as_deref(),
        prefer_live,
        allow_api_key,
        include_metadata_only,
        limit,
    );
    println!("{}", serde_json::to_string_pretty(&json!(result))?);
    Ok(())
}

fn query_source_summary(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut source: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--source" => {
                i += 1;
                source = args.get(i).cloned();
            }
            unknown => return Err(format!("unknown option: {unknown}").into()),
        }
        i += 1;
    }
    let source = source.ok_or("--source is required")?;
    let caps = capability_map();
    let result = source_summary(&caps, &source);
    println!("{}", serde_json::to_string_pretty(&json!(result))?);
    Ok(())
}

fn query_dataset_summary(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut dataset: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dataset" => {
                i += 1;
                dataset = args.get(i).cloned();
            }
            unknown => return Err(format!("unknown option: {unknown}").into()),
        }
        i += 1;
    }
    let dataset = dataset.ok_or("--dataset is required")?;
    let caps = capability_map();
    let result = dataset_summary(&caps, &dataset);
    println!("{}", serde_json::to_string_pretty(&json!(result))?);
    Ok(())
}

fn query_dataset_matrix() -> Result<(), Box<dyn std::error::Error>> {
    let caps = capability_map();
    let result = dataset_source_matrix(&caps);
    println!("{}", serde_json::to_string_pretty(&json!(result))?);
    Ok(())
}

fn recommend_sources(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut use_case: Option<String> = None;
    let mut prefer_live = true;
    let mut allow_api_key = true;
    let mut limit: Option<usize> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--use-case" => {
                i += 1;
                use_case = args.get(i).cloned();
            }
            "--limit" => {
                i += 1;
                let parsed = args.get(i).ok_or("--limit requires a value")?;
                limit = Some(parsed.parse::<usize>()?);
            }
            "--disallow-api-key" => allow_api_key = false,
            "--no-prefer-live" => prefer_live = false,
            unknown => return Err(format!("unknown option: {unknown}").into()),
        }
        i += 1;
    }

    let use_case = use_case.ok_or("--use-case is required")?;
    let caps = capability_map();
    let result =
        recommend_sources_for_use_case(&caps, &use_case, allow_api_key, prefer_live, limit);
    println!("{}", serde_json::to_string_pretty(&json!(result))?);
    Ok(())
}

#[derive(Debug, Default)]
struct IngestOptions {
    source: String,
    symbol: String,
    datasets: Vec<String>,
    asset_type: String,
    timeframe: String,
    limit: usize,
    store: bool,
    record_root: Option<String>,
    manifest_root: Option<String>,
    duckdb_path: Option<String>,
}

fn ingest(
    options: IngestOptions,
    raw_input_override: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Determine storage backend. If a duckdb import is requested and no explicit record_root
    // was given, persist records to a timestamped local store so they can be imported.
    let mut record_root_used: Option<String> = options.record_root.clone();
    let storage: Box<dyn market_data::StorageBackend> = if let Some(record_root) = &record_root_used {
        Box::new(LocalArtifactStorage::new(record_root)) as Box<dyn market_data::StorageBackend>
    } else if options.duckdb_path.is_some() {
        let store_path = format!("artifacts/store/{}", Utc::now().timestamp_millis());
        std::fs::create_dir_all(&store_path)?;
        record_root_used = Some(store_path.clone());
        Box::new(LocalArtifactStorage::new(&store_path)) as Box<dyn market_data::StorageBackend>
    } else {
        Box::new(InMemoryStorage::default()) as Box<dyn market_data::StorageBackend>
    };
    let provenance = ManifestProvenanceTracker::new(options.manifest_root.as_deref());
    let mut registry = SourceAdapterRegistry::default();
    market_data::providers::register_live_providers(&mut registry);
    // If the caller explicitly requested TEFAS as the source (e.g. --source tefas_public),
    // register the TEFAS adapter even when ENABLE_TEFAS is not set. This lets explicit
    // invocations work without requiring the opt-in environment variable.
    if options.source == "tefas" || options.source == "tefas_public" {
        if registry.get("tefas_public").is_none() {
            #[cfg(feature = "tefas")]
            {
                let tefas_adapter = std::sync::Arc::new(market_data::providers::tefas::TefasAdapter::default());
                registry.register("tefas", tefas_adapter.clone());
                registry.register("tefas_public", tefas_adapter);
            }

            #[cfg(not(feature = "tefas"))]
            {
                eprintln!(
                    "Requested TEFAS source ({}), but crate was compiled without the `tefas` feature. Rebuild with `--features tefas` to enable.",
                    options.source
                );
            }
        }
    }
    let mut streaming_registry = market_data::streaming::StreamingAdapterRegistry::default();
    // Register the tradingview streaming POC adapter (writes synthetic ticks to artifacts/streams)
    streaming_registry.register(
        "tradingview",
        std::sync::Arc::new(market_data::providers::tradingview_ws::TradingViewStreamingAdapter::new()),
    );
    let mut hub = DataHub::with_components(storage, provenance, registry, streaming_registry);
    let raw_input = if let Some(raw_input_override) = raw_input_override {
        raw_input_override
    } else {
        let mut raw_input = String::new();
        io::stdin().read_to_string(&mut raw_input)?;
        raw_input
    };
    let result = if raw_input.trim().is_empty() {
        let mut result = hub.ingest(
            &options.source,
            &options.symbol,
            options.datasets,
            &options.timeframe,
            options.limit,
            options.store,
        )?;
        if options.asset_type != "multi_asset" {
            for record in &mut result.records {
                record.asset_type = options.asset_type.clone();
            }
        }
        result
    } else {
        let raw_datasets: HashMap<String, Value> = serde_json::from_str(&raw_input)?;
        hub.ingest_from_raw_with_asset_type(
            &options.source,
            &options.symbol,
            options.datasets,
            raw_datasets,
            options.store,
            &options.asset_type,
        )?
    };

    print_json(&serde_json::to_value(result)?, false)?;
    // If duckdb import was requested, write a helper and attempt to import via python3
    if let Some(duckdb_path) = options.duckdb_path.as_deref() {
        if let Some(record_root_dir) = record_root_used.as_deref() {
            std::fs::create_dir_all("artifacts")?;
            let script_path = "artifacts/duckdb_import.py";
            let script = r###"#!/usr/bin/env python3
        import sys, os
        def main():
            if len(sys.argv) < 3:
                print("usage: duckdb_import.py <duckdb_db> <records_dir>", file=sys.stderr)
                sys.exit(2)
            db_path = sys.argv[1]
            records_dir = sys.argv[2]
            try:
                import duckdb
            except Exception:
                print("duckdb_module_missing", file=sys.stderr)
                sys.exit(3)
            # accept both .json and .jsonl artifact files
            pattern = os.path.join(records_dir, "*.json*")
            try:
                con = duckdb.connect(db_path)
                con.execute(f"CREATE TABLE IF NOT EXISTS imported AS SELECT * FROM read_json_auto('{pattern}')")
                print("import_ok")
            except Exception as e:
                print(f"import_error:{e}", file=sys.stderr)
                sys.exit(1)
        if __name__ == '__main__':
            main()
        "###;
            std::fs::write(script_path, script)?;

            // Only attempt the import if python3 and the duckdb module are available.
            match Command::new("python3").arg("-c").arg("import duckdb").status() {
                Ok(status) if status.success() => {
                    match Command::new("python3").arg(script_path).arg(duckdb_path).arg(record_root_dir).output() {
                        Ok(out) => {
                            if out.status.success() {
                                eprintln!("duckdb import succeeded: {}", String::from_utf8_lossy(&out.stdout));
                            } else {
                                eprintln!("duckdb import failed: {}", String::from_utf8_lossy(&out.stderr));
                                eprintln!("helper written to artifacts/duckdb_import.py");
                            }
                        }
                        Err(e) => {
                            eprintln!("failed to spawn python3 for duckdb import: {}", e);
                            eprintln!("helper written to artifacts/duckdb_import.py");
                        }
                    }
                }
                _ => {
                    // Python duckdb module not available; try system duckdb CLI as a fallback
                    let pattern = format!("{}/{}", record_root_dir, "*.json*");
                    let sql = format!("CREATE TABLE IF NOT EXISTS imported AS SELECT * FROM read_json_auto('{}')", pattern);
                    match Command::new("duckdb").arg(duckdb_path).arg("-c").arg(sql).output() {
                        Ok(out) => {
                            if out.status.success() {
                                eprintln!("duckdb CLI import succeeded: {}", String::from_utf8_lossy(&out.stdout));
                            } else {
                                eprintln!("duckdb CLI import failed: {}", String::from_utf8_lossy(&out.stderr));
                                eprintln!("python3 duckdb module not available; helper written to artifacts/duckdb_import.py");
                            }
                        }
                        Err(e) => {
                            eprintln!("duckdb CLI not available or failed to spawn: {}; helper written to artifacts/duckdb_import.py", e);
                        }
                    }
                }
            }
        } else {
            eprintln!("duckdb path specified but no local records present to import");
        }
    }

    Ok(())
}

fn consume_streams_command(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut dir = "artifacts/streams".to_string();
    let mut store = true;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dir" => {
                i += 1;
                dir = args.get(i).cloned().ok_or("--dir requires a value")?;
            }
            "--no-store" => {
                store = false;
            }
            unknown => return Err(format!("unknown option: {unknown}").into()),
        }
        i += 1;
    }

    let storage = Box::new(InMemoryStorage::default()) as Box<dyn market_data::StorageBackend>;
    let provenance = ManifestProvenanceTracker::new(None::<&str>);
    let mut registry = SourceAdapterRegistry::default();
    market_data::providers::register_live_providers(&mut registry);
    let mut streaming_registry = market_data::streaming::StreamingAdapterRegistry::default();
    streaming_registry.register(
        "tradingview",
        std::sync::Arc::new(market_data::providers::tradingview_ws::TradingViewStreamingAdapter::new()),
    );
    let mut hub = DataHub::with_components(storage, provenance, registry, streaming_registry);

    let count = market_data::stream_consumer::consume_stream_files(&mut hub, &dir, store)?;
    println!("processed {} stream files", count);
    Ok(())
}

fn stream_start_command(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut source = "tradingview".to_string();
    let mut symbol = "AAPL".to_string();
    let mut datasets: Vec<String> = vec!["tick".to_string()];

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--source" => { i += 1; source = args.get(i).cloned().ok_or("--source requires a value")?; }
            "--symbol" => { i += 1; symbol = args.get(i).cloned().ok_or("--symbol requires a value")?; }
            "--datasets" => { i += 1; let v = args.get(i).cloned().ok_or("--datasets requires a value")?; datasets = v.split(',').map(|s| s.trim().to_string()).collect(); }
            unknown => return Err(format!("unknown option: {unknown}").into()),
        }
        i += 1;
    }

    let storage = Box::new(InMemoryStorage::default()) as Box<dyn market_data::StorageBackend>;
    let provenance = ManifestProvenanceTracker::new(None::<&str>);
    let mut registry = SourceAdapterRegistry::default();
    market_data::providers::register_live_providers(&mut registry);
    let mut streaming_registry = market_data::streaming::StreamingAdapterRegistry::default();
    streaming_registry.register(
        "tradingview",
        std::sync::Arc::new(market_data::providers::tradingview_ws::TradingViewStreamingAdapter::new()),
    );
    let mut hub = DataHub::with_components(storage, provenance, registry, streaming_registry);

    hub.start_stream(&source, &symbol, datasets)?;
    println!("{{\"status\":\"started\",\"source\":\"{}\",\"symbol\":\"{}\"}}", source, symbol);
    Ok(())
}

fn stream_stop_command(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut source = "tradingview".to_string();
    let mut symbol = "AAPL".to_string();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--source" => { i += 1; source = args.get(i).cloned().ok_or("--source requires a value")?; }
            "--symbol" => { i += 1; symbol = args.get(i).cloned().ok_or("--symbol requires a value")?; }
            unknown => return Err(format!("unknown option: {unknown}").into()),
        }
        i += 1;
    }

    let storage = Box::new(InMemoryStorage::default()) as Box<dyn market_data::StorageBackend>;
    let provenance = ManifestProvenanceTracker::new(None::<&str>);
    let mut registry = SourceAdapterRegistry::default();
    market_data::providers::register_live_providers(&mut registry);
    let mut streaming_registry = market_data::streaming::StreamingAdapterRegistry::default();
    streaming_registry.register(
        "tradingview",
        std::sync::Arc::new(market_data::providers::tradingview_ws::TradingViewStreamingAdapter::new()),
    );
    let mut hub = DataHub::with_components(storage, provenance, registry, streaming_registry);

    hub.stop_stream(&source, &symbol)?;
    println!("{{\"status\":\"stopped\",\"source\":\"{}\",\"symbol\":\"{}\"}}", source, symbol);
    Ok(())
}

fn smoke_command(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    // If `--all` is passed, iterate all live-capable sources (skip API-key-only sources)
    let mut run_all = false;
    for a in &args { if a == "--all" { run_all = true; } }

    // Run a small set of live checks for representative sources (or all when requested).
    // Fail if any check produces zero coverage or the streaming pipeline produces zero processed files.
    let mut summary: Vec<serde_json::Value> = Vec::new();

    // 1) TEFAS public check (AC5)
    let opts = IngestOptions { source: "tefas_public".to_string(), symbol: "AC5".to_string(), datasets: vec!["tick".to_string(), "kline".to_string()], asset_type: "multi_asset".to_string(), timeframe: "1m".to_string(), limit: 200, store: false, record_root: None, manifest_root: None, duckdb_path: None };
    match run_ingest_with_live_fetch(opts, None) {
        Ok((result, _meta)) => {
            summary.push(json!({"source":"tefas_public","symbol":"AC5","dataset_coverage": result.dataset_coverage, "source_issues": result.source_issues}));
        }
        Err(e) => {
            return Err(format!("tefas_public smoke check failed: {}", e).into());
        }
    }

    // 2) Yahoo unofficial (AAPL)
    let opts2 = IngestOptions { source: "yahoo_unofficial".to_string(), symbol: "AAPL".to_string(), datasets: vec!["kline".to_string(), "tick".to_string()], asset_type: "multi_asset".to_string(), timeframe: "1d".to_string(), limit: 100, store: false, record_root: None, manifest_root: None, duckdb_path: None };
    match run_ingest_with_live_fetch(opts2, None) {
        Ok((result, _meta)) => {
            summary.push(json!({"source":"yahoo_unofficial","symbol":"AAPL","dataset_coverage": result.dataset_coverage, "source_issues": result.source_issues}));
        }
        Err(e) => {
            return Err(format!("yahoo_unofficial smoke check failed: {}", e).into());
        }
    }

    // 3) TradingView streaming check: start adapter, wait briefly, consume stream files
    let storage = Box::new(InMemoryStorage::default()) as Box<dyn market_data::StorageBackend>;
    let provenance = ManifestProvenanceTracker::new(None::<&str>);
    let mut registry = SourceAdapterRegistry::default();
    market_data::providers::register_live_providers(&mut registry);
    let mut streaming_registry = market_data::streaming::StreamingAdapterRegistry::default();
    streaming_registry.register(
        "tradingview",
        std::sync::Arc::new(market_data::providers::tradingview_ws::TradingViewStreamingAdapter::new()),
    );
    let mut hub = DataHub::with_components(storage, provenance, registry, streaming_registry);

    hub.start_stream("tradingview", "AAPL", vec!["tick".to_string()])?;
    // allow the synthetic streaming adapter to produce a few ticks
    std::thread::sleep(std::time::Duration::from_secs(4));
    let processed = market_data::stream_consumer::consume_stream_files(&mut hub, "artifacts/streams", false)?;
    hub.stop_stream("tradingview", "AAPL")?;
    summary.push(json!({"source":"tradingview","symbol":"AAPL","processed_files": processed}));

    println!("{}", serde_json::to_string_pretty(&json!(summary))?);

    // Verify results: ensure at least one data row or processed file was produced
    let mut ok = true;
    for item in &summary {
        if let Some(obj) = item.as_object() {
            if obj.contains_key("processed_files") {
                if obj.get("processed_files").and_then(Value::as_u64).unwrap_or(0) == 0 {
                    ok = false;
                }
            } else if let Some(dc) = obj.get("dataset_coverage") {
                if dc.as_object().map(|m| m.values().all(|v| v.as_u64().unwrap_or(0) == 0)).unwrap_or(true) {
                    ok = false;
                }
            }
        }
    }

    if !ok {
        return Err("smoke checks detected missing data; inspect artifacts for details".into());
    }

    // If requested, iterate other live-capable sources and run a lightweight fetch
    if run_all {
        let caps = capability_map();
        for (source, cap) in caps {
            if cap.requires_api_key {
                // skip sources that require API keys unless env var exists
                if let Some(env_name) = &cap.api_key_env {
                    if std::env::var(env_name).unwrap_or_default().trim().is_empty() {
                        summary.push(json!({"source": source, "skipped": "api_key_missing"}));
                        continue;
                    }
                } else {
                    summary.push(json!({"source": source, "skipped": "api_key_required"}));
                    continue;
                }
            }

            // pick first implemented dataset and a sample symbol
            let dataset = cap.implemented_datasets.first().cloned().unwrap_or_else(|| "kline".to_string());
            let symbol = discover_assets_live(&source, 1).first().cloned().unwrap_or_else(|| "BTCUSDT".to_string());
            let opts = IngestOptions { source: source.clone(), symbol: symbol.clone(), datasets: vec![dataset.clone()], asset_type: "multi_asset".to_string(), timeframe: "1m".to_string(), limit: 50, store: false, record_root: None, manifest_root: None, duckdb_path: None };
            match run_ingest_with_live_fetch(opts, None) {
                Ok((result, _meta)) => {
                    summary.push(json!({"source": source, "symbol": symbol, "dataset": dataset, "dataset_coverage": result.dataset_coverage}));
                }
                Err(e) => {
                    summary.push(json!({"source": source, "error": format!("{}", e)}));
                }
            }
        }
        println!("full_smoke_summary: {}", serde_json::to_string_pretty(&json!(summary))?);
    }

    Ok(())
}

fn run_ingest_with_live_fetch(
    options: IngestOptions,
    fetch_options: Option<&Value>,
) -> Result<(market_data::IngestResult, Map<String, Value>), Box<dyn std::error::Error>> {
    // Determine storage backend and expose the record_root used in metadata.
    let mut record_root_used: Option<String> = options.record_root.clone();
    let storage: Box<dyn market_data::StorageBackend> = if let Some(record_root) = &record_root_used {
        Box::new(LocalArtifactStorage::new(record_root)) as Box<dyn market_data::StorageBackend>
    } else if options.duckdb_path.is_some() {
        let store_path = format!("artifacts/store/{}", Utc::now().timestamp_millis());
        std::fs::create_dir_all(&store_path)?;
        record_root_used = Some(store_path.clone());
        Box::new(LocalArtifactStorage::new(&store_path)) as Box<dyn market_data::StorageBackend>
    } else {
        Box::new(InMemoryStorage::default()) as Box<dyn market_data::StorageBackend>
    };
    let provenance = ManifestProvenanceTracker::new(options.manifest_root.as_deref());
    let mut registry = SourceAdapterRegistry::default();
    market_data::providers::register_live_providers(&mut registry);
    let mut streaming_registry = market_data::streaming::StreamingAdapterRegistry::default();
    streaming_registry.register(
        "tradingview",
        std::sync::Arc::new(market_data::providers::tradingview_ws::TradingViewStreamingAdapter::new()),
    );
    let mut hub = DataHub::with_components(storage, provenance, registry, streaming_registry);
    let mut extra_issues: Vec<String> = Vec::new();

    let mut metadata: Map<String, Value> = Map::new();

    // If duckdb_path is set, force persisting records to storage so they can be imported.
    let effective_store: bool = options.store || options.duckdb_path.is_some();

    let raw_datasets = fetch_options
        .and_then(|value| value.get("raw_datasets"))
        .and_then(Value::as_object)
        .cloned()
        .map(|value| value.into_iter().collect::<HashMap<String, Value>>())
        .unwrap_or_default();

    let mut result = if raw_datasets.is_empty() {
        // Auto-selection / fallback when source is empty or explicitly set to "auto"
        if options.source.is_empty() || options.source == "auto" {
            let caps = capability_map();
            let asset_class = if options.asset_type != "multi_asset" && !options.asset_type.is_empty() {
                Some(options.asset_type.as_str())
            } else {
                None
            };
            let primary_dataset = options
                .datasets
                .first()
                .cloned()
                .unwrap_or_else(|| "kline".to_string());

            let candidates = best_sources_for(
                &caps,
                &primary_dataset,
                asset_class,
                true,
                true,
                false,
                None,
            );

            let mut attempted_sources: Vec<Value> = Vec::new();
            let mut fallback_reasons: Vec<Value> = Vec::new();
            let mut selected_source = String::new();
            let mut fetched_map: HashMap<String, Value> = HashMap::new();

            for row in candidates {
                if let Some(candidate) = row.get("source") {
                    let candidate = candidate.clone();
                    // ensure candidate supports all requested datasets
                    if let Some(cap) = caps.get(candidate.as_str()) {
                        let mut supports_all = true;
                        for ds in &options.datasets {
                            let canonical = market_data::canonical_dataset_name(ds).to_string();
                            if !cap.implemented_datasets.iter().any(|d| d == &canonical) {
                                supports_all = false;
                                break;
                            }
                        }
                        if !supports_all {
                            continue;
                        }

                        attempted_sources.push(Value::String(candidate.clone()));
                        let (fetched, issues) = fetch_live_raw_datasets(
                            &candidate,
                            &options.symbol,
                            &options.datasets,
                            &options.timeframe,
                            options.limit,
                        );
                        if !issues.is_empty() {
                            fallback_reasons.push(Value::String(format!("{}:{}", candidate, issues.join("|"))));
                        }
                        extra_issues.extend(issues.clone());
                        if !fetched.is_empty() {
                            selected_source = candidate.clone();
                            fetched_map = fetched;
                            break;
                        }
                    }
                }
            }

            metadata.insert("selection_mode".to_string(), json!("auto"));
            metadata.insert("attempted_sources".to_string(), Value::Array(attempted_sources.clone()));
            metadata.insert("fallback_reasons".to_string(), Value::Array(fallback_reasons.clone()));
            metadata.insert("warnings".to_string(), json!(extra_issues.clone()));

            if selected_source.is_empty() {
                metadata.insert("selected_source".to_string(), json!("offline_fallback"));
                metadata.insert("fallback_used".to_string(), json!(true));
                hub.ingest(
                    "offline_fallback",
                    &options.symbol,
                    options.datasets,
                    &options.timeframe,
                    options.limit,
                    effective_store,
                )?
            } else {
                metadata.insert("selected_source".to_string(), json!(selected_source.clone()));
                metadata.insert("fallback_used".to_string(), json!(true));
                hub.ingest_from_raw_with_asset_type(
                    &selected_source,
                    &options.symbol,
                    options.datasets,
                    fetched_map,
                    effective_store,
                    &options.asset_type,
                )?
            }
        } else {
            // explicit source provided: behave as before
            let (fetched, issues) = fetch_live_raw_datasets(
                &options.source,
                &options.symbol,
                &options.datasets,
                &options.timeframe,
                options.limit,
            );
            extra_issues.extend(issues.clone());

            metadata.insert("selection_mode".to_string(), json!("explicit"));
            metadata.insert("selected_source".to_string(), json!(options.source.clone()));
            metadata.insert("attempted_sources".to_string(), Value::Array(vec![json!(options.source.clone())]));
            metadata.insert("fallback_used".to_string(), json!(false));
            metadata.insert("fallback_reasons".to_string(), Value::Array(Vec::new()));
            metadata.insert("warnings".to_string(), json!(extra_issues.clone()));

            if fetched.is_empty() && (options.source == "offline" || options.source == "offline_fallback") {
                hub.ingest(
                    &options.source,
                    &options.symbol,
                    options.datasets,
                    &options.timeframe,
                    options.limit,
                    effective_store,
                )?
            } else {
                hub.ingest_from_raw_with_asset_type(
                    &options.source,
                    &options.symbol,
                    options.datasets,
                    fetched,
                    effective_store,
                    &options.asset_type,
                )?
            }
        }
    } else {
        hub.ingest_from_raw_with_asset_type(
            &options.source,
            &options.symbol,
            options.datasets,
            raw_datasets,
            effective_store,
            &options.asset_type,
        )?
    };


    // If duckdb import requested, and we have a persisted record_root (set above), write helper and attempt import
    if let Some(duckdb_path) = options.duckdb_path.as_deref() {
        if let Some(record_root_dir) = options.record_root.as_ref().or(record_root_used.as_ref()) {
            std::fs::create_dir_all("artifacts")?;
            let script_path = "artifacts/duckdb_import.py";
            let script = r###"#!/usr/bin/env python3
import sys, os
def main():
    if len(sys.argv) < 3:
        print("usage: duckdb_import.py <duckdb_db> <records_dir>", file=sys.stderr)
        sys.exit(2)
    db_path = sys.argv[1]
    records_dir = sys.argv[2]
    try:
        import duckdb
    except Exception:
        print("duckdb_module_missing", file=sys.stderr)
        sys.exit(3)
    # accept both .json and .jsonl artifact files
    pattern = os.path.join(records_dir, "*.json*")
    try:
        con = duckdb.connect(db_path)
        con.execute(f"CREATE TABLE IF NOT EXISTS imported AS SELECT * FROM read_json_auto('{pattern}')")
        print("import_ok")
    except Exception as e:
        print(f"import_error:{e}", file=sys.stderr)
        sys.exit(1)
if __name__ == '__main__':
    main()
"###;
            std::fs::write(script_path, script)?;

            match Command::new("python3").arg("-c").arg("import duckdb").status() {
                Ok(status) if status.success() => {
                    match Command::new("python3").arg(script_path).arg(duckdb_path).arg(record_root_dir).output() {
                        Ok(out) => {
                            if out.status.success() {
                                eprintln!("duckdb import succeeded: {}", String::from_utf8_lossy(&out.stdout));
                            } else {
                                eprintln!("duckdb import failed: {}", String::from_utf8_lossy(&out.stderr));
                            }
                        }
                        Err(e) => {
                            eprintln!("failed to spawn python3 for duckdb import: {}", e);
                        }
                    }
                }
                _ => {
                    eprintln!("python3 duckdb module not available; helper written to artifacts/duckdb_import.py");
                }
            }
        } else {
            eprintln!("duckdb path specified but no local records present to import");
        }
    }
    let source_for_issues = metadata
        .get("selected_source")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| options.source.clone());

    for reason in extra_issues {
        result.source_issues.push(std::collections::BTreeMap::from([
            ("source".to_string(), source_for_issues.clone()),
            ("reason".to_string(), reason),
        ]));
    }
    // Expose the record root used (if any) in the metadata for callers
    metadata.insert("record_root".to_string(), json!(record_root_used));
    Ok((result, metadata))
}

fn ingest_options_from_json_payload(
    payload: &Map<String, Value>,
) -> Result<IngestOptions, Box<dyn std::error::Error>> {
    let source = payload_string(payload, "source").ok_or("source is required")?;
    let symbol = payload_string(payload, "symbol").ok_or("symbol is required")?;
    let datasets = payload
        .get("datasets")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if datasets.is_empty() {
        return Err("datasets is required".into());
    }

    Ok(IngestOptions {
        source,
        symbol,
        datasets,
        asset_type: payload_string(payload, "asset_type").unwrap_or_else(|| "multi_asset".to_string()),
        timeframe: payload_string(payload, "timeframe").unwrap_or_else(|| "1m".to_string()),
        limit: payload
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(500),
        store: payload_bool(payload, "store").unwrap_or(false),
        record_root: payload_string(payload, "record_root"),
        manifest_root: payload_string(payload, "manifest_root"),
        duckdb_path: payload_string(payload, "duckdb").or_else(|| payload_string(payload, "duckdb_path")),
    })
}

fn load_market_data_options_from_json_payload(
    payload: &Map<String, Value>,
) -> Result<IngestOptions, Box<dyn std::error::Error>> {
    let source = payload_string(payload, "source").ok_or("source is required")?;
    let symbol = payload_string(payload, "symbol").ok_or("symbol is required")?;
    let dataset = payload_string(payload, "dataset").ok_or("dataset is required")?;
    Ok(IngestOptions {
        source,
        symbol,
        datasets: vec![dataset],
        asset_type: "multi_asset".to_string(),
        timeframe: payload_string(payload, "timeframe").unwrap_or_else(|| "1m".to_string()),
        limit: payload
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(500),
        store: false,
        record_root: None,
        manifest_root: None,
        duckdb_path: None,
    })
}

fn discover_assets_live(source: &str, limit: usize) -> Vec<String> {
    let max_limit = limit.max(1);
    match source {
        "binance_futures" => discover_binance_futures_assets(max_limit),
        "binance_spot" => discover_binance_spot_assets(max_limit),
        "bybit_linear" => discover_bybit_assets(max_limit),
        "coingecko" => discover_coingecko_assets(max_limit),
        "coinbase_spot" => discover_coinbase_assets(max_limit),
        "ecb" => vec!["USD".to_string(), "GBP".to_string(), "JPY".to_string()]
            .into_iter()
            .take(max_limit)
            .collect(),
        "stooq" => vec!["aapl.us".to_string(), "msft.us".to_string(), "spy.us".to_string()]
            .into_iter()
            .take(max_limit)
            .collect(),
        "frankfurter_fx" => discover_frankfurter_assets(max_limit),
        "gdelt" => vec!["bitcoin".to_string(), "fed".to_string(), "oil".to_string()]
            .into_iter()
            .take(max_limit)
            .collect(),
        "hacker_news" => vec!["BITCOIN".to_string(), "ETHEREUM".to_string()]
            .into_iter()
            .take(max_limit)
            .collect(),
        "kraken_spot" => discover_kraken_assets(max_limit),
        "world_bank" => vec!["NY.GDP.MKTP.CD".to_string(), "FP.CPI.TOTL.ZG".to_string()]
            .into_iter()
            .take(max_limit)
            .collect(),
        "yahoo_unofficial" => vec!["AAPL".to_string(), "MSFT".to_string(), "BTC-USD".to_string()]
            .into_iter()
            .take(max_limit)
            .collect(),
        "offline" | "offline_fallback" => vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]
            .into_iter()
            .take(max_limit)
            .collect(),
        _ => Vec::new(),
    }
}

fn fetch_live_raw_datasets(
    source: &str,
    symbol: &str,
    datasets: &[String],
    timeframe: &str,
    limit: usize,
) -> (HashMap<String, Value>, Vec<String>) {
    if source == "offline" || source == "offline_fallback" {
        return (HashMap::new(), Vec::new());
    }

    let caps = capability_map();
    let mut issues = Vec::new();
    let mut fetchable = Vec::new();

    // Allow common CLI-friendly aliases to map to canonical capability names.
    let mut source_key = source.to_string();
    if !caps.contains_key(source_key.as_str()) {
        match source {
            "yahoo" => source_key = "yahoo_unofficial".to_string(),
            "tefas" => source_key = "tefas_public".to_string(),
            _ => {}
        }
    }

    if let Some(cap) = caps.get(source_key.as_str()) {
        if cap.requires_api_key
            && let Some(env_name) = &cap.api_key_env
            && env::var(env_name).unwrap_or_default().trim().is_empty()
        {
            issues.push(format!("api_key_required:{env_name}"));
            return (HashMap::new(), issues);
        }

        for dataset in datasets {
            let canonical = market_data::canonical_dataset_name(dataset).to_string();
                if !cap.datasets.iter().any(|item| item == &canonical)
                    || !cap.implemented_datasets.iter().any(|item| item == &canonical)
                {
                    issues.push(format!("unsupported_dataset:{canonical}"));
                    continue;
                }
            fetchable.push(canonical);
        }
    } else {
        issues.push("unknown_source".to_string());
        return (HashMap::new(), issues);
    }

    let mut out = HashMap::new();
    for dataset in fetchable {
        match fetch_live_dataset(source_key.as_str(), symbol, &dataset, timeframe, limit) {
            Ok(value) => {
                out.insert(dataset, value);
            }
            Err(reason) => {
                issues.push(reason);
            }
        }
    }
    (out, issues)
}

fn fetch_live_dataset(
    source: &str,
    symbol: &str,
    dataset: &str,
    timeframe: &str,
    limit: usize,
) -> Result<Value, String> {
    match (source, dataset) {
        ("binance_futures", "tick") => fetch_binance_futures_tick(symbol),
        ("binance_spot", "tick") => fetch_binance_spot_tick(symbol),
        ("coingecko", "kline") => fetch_coingecko_kline(symbol, limit),
        ("coingecko", "tick") => fetch_coingecko_tick(symbol),
        ("coinbase_spot", "orderbook") => fetch_coinbase_orderbook(symbol),
        ("stooq", "kline") => fetch_stooq_kline(symbol),
        ("gdelt", "news") => fetch_gdelt_news(symbol, limit),
        ("yahoo_unofficial", "kline") => fetch_yahoo_kline(symbol, timeframe, limit),
        ("yahoo_unofficial", "tick") => fetch_yahoo_tick(symbol),
        ("frankfurter_fx", "macro") => fetch_frankfurter_macro(symbol),
        ("frankfurter_fx", "tick") => fetch_frankfurter_tick(symbol),
        ("ecb", "macro") => fetch_ecb_macro(symbol),
        ("ecb", "tick") => fetch_ecb_tick(symbol),
        ("world_bank", "macro") => fetch_world_bank_macro(symbol),
        ("hacker_news", "news") => fetch_hacker_news(symbol, limit),
        ("kraken_spot", "kline") => fetch_kraken_kline(symbol, timeframe, limit),
        ("kraken_spot", "tick") => fetch_kraken_tick(symbol),
        ("kraken_spot", "trade") => fetch_kraken_trade(symbol, limit),
        ("kraken_spot", "orderbook") => fetch_kraken_orderbook(symbol),
        ("btcturk", "tick") => fetch_btcturk_tick(symbol),
        ("paratic", "tick") => fetch_paratic_tick(symbol),
        ("dovizcom", "tick") => fetch_dovizcom_tick(symbol),
        ("kap", "news") => fetch_kap_disclosures(symbol),
        ("kap", "fundamentals") => fetch_kap_disclosures(symbol),
        ("kap", "corporate_actions") => fetch_kap_disclosures(symbol),
        ("fintables", "fundamentals") => fetch_fintables_fundamentals(symbol),
        ("fintables", "corporate_actions") => fetch_fintables_fundamentals(symbol),
        ("tefas_public", "fundamentals") => fetch_tefas_values(symbol),
        ("tefas_public", "corporate_actions") => fetch_tefas_values(symbol),
        ("coinbase_spot", "kline") => fetch_coinbase_kline(symbol, timeframe, limit),
        ("coinbase_spot", "tick") => fetch_coinbase_tick(symbol),
        ("coinbase_spot", "trade") => fetch_coinbase_trade(symbol, limit),
        ("bybit_linear", "kline") => fetch_bybit_kline(symbol, timeframe, limit),
        ("bybit_linear", "funding") => fetch_bybit_funding(symbol, limit),
        ("bybit_linear", "tick") => fetch_bybit_tick(symbol),
        ("bybit_linear", "trade") => fetch_bybit_trade(symbol, limit),
        ("bybit_linear", "orderbook") => fetch_bybit_orderbook(symbol),
        ("binance_futures", "kline") => fetch_binance_futures_kline(symbol, timeframe, limit),
        ("binance_futures", "funding") => fetch_binance_futures_funding(symbol, limit),
        ("binance_futures", "trade") => fetch_binance_futures_trade(symbol, limit),
        ("binance_futures", "orderbook") => fetch_binance_futures_orderbook(symbol),
        ("binance_spot", "kline") => fetch_binance_spot_kline(symbol, timeframe, limit),
        ("binance_spot", "trade") => fetch_binance_spot_trade(symbol, limit),
        ("binance_spot", "orderbook") => fetch_binance_spot_orderbook(symbol),
        // Generic TEFAS fallback: attempt TEFAS values endpoint for fund datasets
        ("tefas_public", "kline") => fetch_tefas_kline(symbol),
        ("tefas_public", "tick") => fetch_tefas_tick(symbol),
        (_, _) if source == "tefas_public" => fetch_tefas_values(symbol),
        _ => Err(format!("unsupported_dataset:{dataset}")),
    }
}

fn fetch_btcturk_tick(symbol: &str) -> Result<Value, String> {
    let base = std::env::var("BTCTURK_BASE_URL").unwrap_or_else(|_| "https://api.btcturk.com".to_string());
    let base = base.trim_end_matches('/');
    let url = format!("{}/api/v2/ticker?pairSymbol={}", base, symbol);
    let json_v = fetch_json(&url, "btcturk")?;
    if let Some(arr) = json_v.get("data").and_then(|d| d.as_array()) {
        if let Some(item) = arr.get(0) {
            let mut map = serde_json::Map::new();
            if let Some(last) = item.get("last") {
                map.insert("last".to_string(), last.clone());
            }
            if let Some(bid) = item.get("bid") {
                map.insert("bid".to_string(), bid.clone());
            }
            if let Some(ask) = item.get("ask") {
                map.insert("ask".to_string(), ask.clone());
            }
            if let Some(ts) = item.get("timestamp") {
                map.insert("timestamp_ms".to_string(), ts.clone());
            }
            map.insert("source".to_string(), Value::String("btcturk".to_string()));
            return Ok(Value::Array(vec![Value::Object(map)]));
        }
    }
    Ok(Value::Array(Vec::new()))
}

fn fetch_kap_disclosures(symbol: &str) -> Result<Value, String> {
    let base = std::env::var("KAP_BASE_URL").unwrap_or_else(|_| "https://www.kap.org.tr".to_string());
    let base = base.trim_end_matches('/');
    let url = format!("{}/tr/api/disclosures?company={}", base, symbol);
    let mut json_v = fetch_json(&url, "kap")?;
    if let Some(arr) = json_v.as_array_mut() {
        for item in arr.iter_mut() {
            if let Value::Object(map) = item {
                map.insert("source".to_string(), Value::String("kap".to_string()));
            }
        }
        return Ok(Value::Array(arr.clone()));
    }
    // wrap non-array responses
    Ok(Value::Array(vec![json_v]))
}

fn fetch_fintables_fundamentals(symbol: &str) -> Result<Value, String> {
    let scraping_enabled = std::env::var("ENABLE_SCRAPING_PROVIDERS").unwrap_or_default() == "true";
    if !scraping_enabled {
        return Err("scraping_disabled:fintables".to_string());
    }
    let base = std::env::var("FINTABLES_BASE_URL").unwrap_or_else(|_| "https://fintables.com".to_string());
    let base = base.trim_end_matches('/');
    let url = format!("{}/sirketler/{}/sermaye-artirimlari-temettuler", base, symbol);
    let text = fetch_text(&url, "fintables")?;
    Ok(json!([{"html": text, "source": "fintables", "symbol": symbol}]))
}

fn fetch_paratic_tick(symbol: &str) -> Result<Value, String> {
    let base = std::env::var("PARATIC_BASE_URL").unwrap_or_else(|_| "https://piyasa.paratic.com".to_string());
    let base = base.trim_end_matches('/');
    let url = format!("{}/API/g.php?symbol={}", base, symbol);
    let json_v = fetch_json(&url, "paratic")?;
    Ok(json_v)
}

fn fetch_dovizcom_tick(symbol: &str) -> Result<Value, String> {
    let base = std::env::var("DOVIZCOM_BASE_URL").unwrap_or_else(|_| "https://www.doviz.com".to_string());
    let base = base.trim_end_matches('/');
    let url = format!("{}/api/v1/symbols/{}/ticker", base, symbol);
    let json_v = fetch_json(&url, "dovizcom")?;
    Ok(json_v)
}

fn fetch_tefas_values(symbol: &str) -> Result<Value, String> {
    // Prefer using an external tefas CLI tool when available. The CLI exposes
    // a `query fonFiyatBilgiGetir --set fonKodu=<code> --format json` invocation
    // which returns TEFAS API JSON. Allow overriding the binary via
    // `TEFAS_CLI_CMD` environment variable.
    fn try_run_cli(cmd: &str, args: &[&str]) -> Result<Value, String> {
        let output = Command::new(cmd)
            .args(args)
            .output()
            .map_err(|e| format!("tefas_cli_spawn_error:{e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("tefas_cli_exit:{}:{}", output.status, stderr));
        }
        let stdout = String::from_utf8(output.stdout).map_err(|e| format!("tefas_cli_output_utf8:{e}"))?;
        serde_json::from_str::<Value>(&stdout).map_err(|e| format!("tefas_cli_output_json:{e}"))
    }

    // Build candidate binary names/paths and args
    let possible_bins: Vec<String> = vec![
        std::env::var("TEFAS_CLI_CMD").unwrap_or_default(),
        "tefas-cli".to_string(),
        "cli".to_string(),
        "tefas".to_string(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect();

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

    for bin in possible_bins {
        let arg_slices: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        if let Ok(v) = try_run_cli(&bin, &arg_slices) {
            return Ok(v);
        }
    }

    // Fallback to simple TEFAS public HTTP endpoint if CLI not available or fails
    let base = std::env::var("TEFAS_BASE_URL").unwrap_or_else(|_| "https://www.tefas.gov.tr".to_string());
    let base = base.trim_end_matches('/');
    let url = format!("{}/api/values?symbol={}", base, symbol);
    let json_v = fetch_json(&url, "tefas")?;
    Ok(json_v)
}

fn fetch_tefas_kline(symbol: &str) -> Result<Value, String> {
    // Reuse the existing CLI-first query but synthesise OHLCV kline rows
    let v = fetch_tefas_values(symbol)?;
    if let Some(arr) = v.get("fonFiyatBilgiGetir").and_then(|o| o.get("resultList")).and_then(|r| r.as_array()) {
        let mut out_arr = Vec::new();
        for item in arr {
            if let Some(obj) = item.as_object() {
                let tarih = obj.get("tarih").and_then(|v| v.as_str()).unwrap_or_default();
                let ts_ms = NaiveDate::parse_from_str(tarih, "%Y-%m-%d").ok()
                    .map(|d| d.and_hms_opt(0,0,0).unwrap())
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
        return Ok(Value::Array(out_arr));
    }
    Ok(Value::Array(Vec::new()))
}

fn fetch_tefas_tick(symbol: &str) -> Result<Value, String> {
    let v = fetch_tefas_values(symbol)?;
    if let Some(arr) = v.get("fonFiyatBilgiGetir").and_then(|o| o.get("resultList")).and_then(|r| r.as_array()) {
        if let Some(latest) = arr.last() {
            if let Some(obj) = latest.as_object() {
                let tarih = obj.get("tarih").and_then(|v| v.as_str()).unwrap_or_default();
                let ts_ms = NaiveDate::parse_from_str(tarih, "%Y-%m-%d").ok()
                    .map(|d| d.and_hms_opt(0,0,0).unwrap())
                    .map(|ndt| Utc.from_utc_datetime(&ndt).timestamp_millis())
                    .unwrap_or(0_i64);
                let price = obj.get("fiyat").cloned().or_else(|| obj.get("price").cloned()).unwrap_or(Value::Null);
                let mut rec = serde_json::Map::new();
                rec.insert("timestamp_ms".to_string(), Value::from(ts_ms));
                rec.insert("last".to_string(), price.clone());
                rec.insert("price".to_string(), price);
                return Ok(Value::Array(vec![Value::Object(rec)]));
            }
        }
    }
    Ok(Value::Array(Vec::new()))
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("market_data_bridge/1.0")
        .build()
        .map_err(|error| format!("network_error:http_client:{error}"))
}

fn fetch_json(url: &str, source: &str) -> Result<Value, String> {
    let client = http_client()?;
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("network_error:{source}:{error}"))?;
    let status = response.status();
    if status.as_u16() == 429 {
        return Err(format!("rate_limited:{source}"));
    }
    if !status.is_success() {
        return Err(format!("network_error:{source}:http_{}", status.as_u16()));
    }
    response
        .json::<Value>()
        .map_err(|error| format!("network_error:{source}:{error}"))
}

fn fetch_text(url: &str, source: &str) -> Result<String, String> {
    let client = http_client()?;
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("network_error:{source}:{error}"))?;
    let status = response.status();
    if status.as_u16() == 429 {
        return Err(format!("rate_limited:{source}"));
    }
    if !status.is_success() {
        return Err(format!("network_error:{source}:http_{}", status.as_u16()));
    }
    response
        .text()
        .map_err(|error| format!("network_error:{source}:{error}"))
}

fn timeframe_to_kraken_interval(timeframe: &str) -> i64 {
    match timeframe {
        "1m" => 1,
        "5m" => 5,
        "15m" => 15,
        "30m" => 30,
        "1h" => 60,
        "4h" => 240,
        "1d" => 1_440,
        _ => 1,
    }
}

fn timeframe_to_coinbase_granularity(timeframe: &str) -> i64 {
    match timeframe {
        "1m" => 60,
        "5m" => 300,
        "15m" => 900,
        "1h" => 3_600,
        "6h" => 21_600,
        "1d" => 86_400,
        _ => 60,
    }
}

fn timeframe_to_binance_interval(timeframe: &str) -> &'static str {
    match timeframe {
        "1m" => "1m",
        "5m" => "5m",
        "15m" => "15m",
        "30m" => "30m",
        "1h" => "1h",
        "4h" => "4h",
        "1d" => "1d",
        _ => "1m",
    }
}

fn fetch_coingecko_kline(symbol: &str, limit: usize) -> Result<Value, String> {
    let url = format!(
        "https://api.coingecko.com/api/v3/coins/{symbol}/market_chart?vs_currency=usd&days=1"
    );
    let payload = fetch_json(&url, "coingecko")?;
    let prices = payload
        .get("prices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let volumes = payload
        .get("total_volumes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for (index, row) in prices.iter().enumerate().take(limit.max(1)) {
        let Some(values) = row.as_array() else { continue };
        if values.len() < 2 {
            continue;
        }
        let ts = values[0].as_i64().unwrap_or(0);
        let price = values[1].as_f64().unwrap_or(0.0);
        let volume = volumes
            .get(index)
            .and_then(Value::as_array)
            .and_then(|v| v.get(1))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        out.push(json!([ts, price, price, price, price, volume]));
    }
    Ok(Value::Array(out))
}

fn fetch_coingecko_tick(symbol: &str) -> Result<Value, String> {
    let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={symbol}&vs_currencies=usd");
    let payload = fetch_json(&url, "coingecko")?;
    let price = payload
        .get(symbol)
        .and_then(Value::as_object)
        .and_then(|row| row.get("usd"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    Ok(json!([{
        "timestamp_ms": Utc::now().timestamp_millis(),
        "last": price,
        "bid": price,
        "ask": price,
    }]))
}

fn fetch_stooq_kline(symbol: &str) -> Result<Value, String> {
    let url = format!("https://stooq.com/q/d/l/?s={symbol}&i=d");
    let body = fetch_text(&url, "stooq")?;
    let mut out = Vec::new();
    for line in body.lines().skip(1) {
        let cells: Vec<&str> = line.split(',').collect();
        if cells.len() < 6 {
            continue;
        }
        let ts = NaiveDate::parse_from_str(cells[0], "%Y-%m-%d")
            .ok()
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|dt| Utc.from_utc_datetime(&dt).timestamp_millis())
            .unwrap_or(0);
        if ts == 0 {
            continue;
        }
        out.push(json!([
            ts,
            cells[1].parse::<f64>().unwrap_or(0.0),
            cells[2].parse::<f64>().unwrap_or(0.0),
            cells[3].parse::<f64>().unwrap_or(0.0),
            cells[4].parse::<f64>().unwrap_or(0.0),
            cells[5].parse::<f64>().unwrap_or(0.0)
        ]));
    }
    Ok(Value::Array(out))
}

fn fetch_frankfurter_macro(symbol: &str) -> Result<Value, String> {
    let base = if symbol.trim().is_empty() { "USD" } else { symbol };
    let url = format!("https://api.frankfurter.dev/v1/latest?base={base}");
    let payload = fetch_json(&url, "frankfurter_fx")?;
    let date = payload
        .get("date")
        .and_then(Value::as_str)
        .unwrap_or("1970-01-01");
    let rates = payload
        .get("rates")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for (quote, rate) in rates {
        out.push(json!({
            "date": format!("{date}T00:00:00Z"),
            "series_id": format!("FX_{base}_{quote}"),
            "value": rate,
        }));
    }
    Ok(Value::Array(out))
}

fn fetch_frankfurter_tick(symbol: &str) -> Result<Value, String> {
    let quote = if symbol.trim().is_empty() { "EUR" } else { symbol };
    let url = format!("https://api.frankfurter.dev/v1/latest?base=USD&symbols={quote}");
    let payload = fetch_json(&url, "frankfurter_fx")?;
    let value = payload
        .get("rates")
        .and_then(Value::as_object)
        .and_then(|rates| rates.get(quote))
        .cloned()
        .unwrap_or(Value::from(0.0));
    Ok(json!([{
        "timestamp_ms": Utc::now().timestamp_millis(),
        "last": value,
        "bid": value,
        "ask": value,
    }]))
}

fn fetch_hacker_news(symbol: &str, limit: usize) -> Result<Value, String> {
    let query = if symbol.trim().is_empty() { "bitcoin" } else { symbol };
    let url = format!(
        "https://hn.algolia.com/api/v1/search?query={query}&tags=story&hitsPerPage={}",
        limit.max(1)
    );
    let payload = fetch_json(&url, "hacker_news")?;
    let hits = payload
        .get("hits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let rows = hits
        .into_iter()
        .map(|hit| {
            json!({
                "title": hit.get("title").cloned().unwrap_or(Value::String(String::new())),
                "url": hit
                    .get("url")
                    .cloned()
                    .or_else(|| hit.get("story_url").cloned())
                    .unwrap_or(Value::String(String::new())),
                "publishedAt": hit
                    .get("created_at")
                    .cloned()
                    .unwrap_or(Value::String(Utc::now().to_rfc3339())),
            })
        })
        .collect::<Vec<_>>();
    Ok(Value::Array(rows))
}

fn fetch_gdelt_news(symbol: &str, limit: usize) -> Result<Value, String> {
    let query = if symbol.trim().is_empty() { "bitcoin" } else { symbol };
    let url = format!(
        "https://api.gdeltproject.org/api/v2/doc/doc?query={query}&mode=artlist&format=json&maxrecords={}",
        limit.max(1)
    );
    let payload = fetch_json(&url, "gdelt")?;
    let rows = payload
        .get("articles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            json!({
                "title": row.get("title").cloned().unwrap_or(Value::String(String::new())),
                "url": row.get("url").cloned().unwrap_or(Value::String(String::new())),
                "publishedAt": row
                    .get("seendate")
                    .cloned()
                    .or_else(|| row.get("socialimage").cloned())
                    .unwrap_or(Value::String(Utc::now().to_rfc3339())),
            })
        })
        .collect::<Vec<_>>();
    Ok(Value::Array(rows))
}

fn fetch_yahoo_kline(symbol: &str, timeframe: &str, limit: usize) -> Result<Value, String> {
    let interval = match timeframe {
        "1m" => "1m",
        "5m" => "5m",
        "15m" => "15m",
        "1h" => "60m",
        "1d" => "1d",
        _ => "1d",
    };
    let range = if interval == "1d" { "1y" } else { "7d" };
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval={interval}&range={range}"
    );
    let payload = fetch_json(&url, "yahoo_unofficial")?;
    let result = payload
        .get("chart")
        .and_then(|row| row.get("result"))
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let timestamps = result
        .get("timestamp")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let quote = result
        .get("indicators")
        .and_then(|row| row.get("quote"))
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let opens = quote.get("open").and_then(Value::as_array).cloned().unwrap_or_default();
    let highs = quote.get("high").and_then(Value::as_array).cloned().unwrap_or_default();
    let lows = quote.get("low").and_then(Value::as_array).cloned().unwrap_or_default();
    let closes = quote.get("close").and_then(Value::as_array).cloned().unwrap_or_default();
    let volumes = quote.get("volume").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for index in 0..timestamps.len().min(limit.max(1)) {
        let ts = timestamps.get(index).and_then(Value::as_i64).unwrap_or(0) * 1000;
        if ts <= 0 {
            continue;
        }
        out.push(json!([
            ts,
            opens.get(index).and_then(Value::as_f64).unwrap_or(0.0),
            highs.get(index).and_then(Value::as_f64).unwrap_or(0.0),
            lows.get(index).and_then(Value::as_f64).unwrap_or(0.0),
            closes.get(index).and_then(Value::as_f64).unwrap_or(0.0),
            volumes.get(index).and_then(Value::as_f64).unwrap_or(0.0)
        ]));
    }
    Ok(Value::Array(out))
}

fn fetch_yahoo_tick(symbol: &str) -> Result<Value, String> {
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval=1d&range=1d"
    );
    let payload = fetch_json(&url, "yahoo_unofficial")?;
    let close = payload
        .get("chart")
        .and_then(|row| row.get("result"))
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("indicators"))
        .and_then(|row| row.get("quote"))
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("close"))
        .and_then(Value::as_array)
        .and_then(|rows| rows.last())
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    Ok(json!([{
        "timestamp_ms": Utc::now().timestamp_millis(),
        "last": close,
        "bid": close,
        "ask": close,
    }]))
}

fn fetch_kraken_kline(symbol: &str, timeframe: &str, limit: usize) -> Result<Value, String> {
    let interval = timeframe_to_kraken_interval(timeframe);
    let url = format!(
        "https://api.kraken.com/0/public/OHLC?pair={symbol}&interval={interval}"
    );
    let payload = fetch_json(&url, "kraken_spot")?;
    let result = payload
        .get("result")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let series = result
        .iter()
        .find(|(key, _)| key.as_str() != "last")
        .and_then(|(_, value)| value.as_array().cloned())
        .unwrap_or_default();
    let mut out = Vec::new();
    for row in series.into_iter().take(limit.max(1)) {
        let Some(values) = row.as_array() else { continue };
        if values.len() < 7 {
            continue;
        }
        let ts = values[0].as_i64().unwrap_or(0) * 1000;
        out.push(json!([
            ts,
            values[1].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[2].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[3].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[4].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[6].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0)
        ]));
    }
    Ok(Value::Array(out))
}

fn fetch_coinbase_kline(symbol: &str, timeframe: &str, limit: usize) -> Result<Value, String> {
    let granularity = timeframe_to_coinbase_granularity(timeframe);
    let url = format!(
        "https://api.exchange.coinbase.com/products/{symbol}/candles?granularity={granularity}"
    );
    let payload = fetch_json(&url, "coinbase_spot")?;
    let rows = payload.as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for row in rows.into_iter().take(limit.max(1)) {
        let Some(values) = row.as_array() else { continue };
        if values.len() < 6 {
            continue;
        }
        let ts = values[0].as_i64().unwrap_or(0) * 1000;
        let low = values[1].as_f64().unwrap_or(0.0);
        let high = values[2].as_f64().unwrap_or(0.0);
        let open = values[3].as_f64().unwrap_or(0.0);
        let close = values[4].as_f64().unwrap_or(0.0);
        let volume = values[5].as_f64().unwrap_or(0.0);
        out.push(json!([ts, open, high, low, close, volume]));
    }
    Ok(Value::Array(out))
}

fn fetch_binance_futures_kline(
    symbol: &str,
    timeframe: &str,
    limit: usize,
) -> Result<Value, String> {
    let interval = timeframe_to_binance_interval(timeframe);
    let url = format!(
        "https://fapi.binance.com/fapi/v1/klines?symbol={symbol}&interval={interval}&limit={}",
        limit.max(1)
    );
    let payload = fetch_json(&url, "binance_futures")?;
    let rows = payload.as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for row in rows {
        let Some(values) = row.as_array() else { continue };
        if values.len() < 6 {
            continue;
        }
        out.push(json!([
            values[0],
            values[1].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[2].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[3].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[4].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[5].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0)
        ]));
    }
    Ok(Value::Array(out))
}

fn fetch_binance_futures_funding(symbol: &str, limit: usize) -> Result<Value, String> {
    let url = format!(
        "https://fapi.binance.com/fapi/v1/fundingRate?symbol={symbol}&limit={}",
        limit.max(1)
    );
    let payload = fetch_json(&url, "binance_futures")?;
    let rows = payload.as_array().cloned().unwrap_or_default();
    let out = rows
        .into_iter()
        .map(|row| {
            json!({
                "fundingTime": row.get("fundingTime").cloned().unwrap_or(Value::from(0_i64)),
                "fundingRate": row.get("fundingRate").cloned().unwrap_or(Value::String("0".to_string())),
            })
        })
        .collect::<Vec<_>>();
    Ok(Value::Array(out))
}

fn fetch_binance_futures_tick(symbol: &str) -> Result<Value, String> {
    let url = format!("https://fapi.binance.com/fapi/v1/ticker/bookTicker?symbol={symbol}");
    let payload = fetch_json(&url, "binance_futures")?;
    let bid = payload
        .get("bidPrice")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let ask = payload
        .get("askPrice")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    Ok(json!([{
        "timestamp_ms": Utc::now().timestamp_millis(),
        "bid": bid,
        "ask": ask,
        "last": (bid + ask) / 2.0,
    }]))
}

fn fetch_binance_futures_trade(symbol: &str, limit: usize) -> Result<Value, String> {
    let url = format!(
        "https://fapi.binance.com/fapi/v1/trades?symbol={symbol}&limit={}",
        limit.max(1)
    );
    let payload = fetch_json(&url, "binance_futures")?;
    let rows = payload.as_array().cloned().unwrap_or_default();
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "t": row.get("time").cloned().unwrap_or(Value::from(0_i64)),
                    "price": row.get("price").cloned().unwrap_or(Value::String("0".to_string())),
                    "qty": row.get("qty").cloned().unwrap_or(Value::String("0".to_string())),
                })
            })
            .collect(),
    ))
}

fn fetch_binance_futures_orderbook(symbol: &str) -> Result<Value, String> {
    let url = format!("https://fapi.binance.com/fapi/v1/depth?symbol={symbol}&limit=50");
    fetch_json(&url, "binance_futures")
}

fn fetch_binance_spot_kline(symbol: &str, timeframe: &str, limit: usize) -> Result<Value, String> {
    let interval = timeframe_to_binance_interval(timeframe);
    let url = format!(
        "https://api.binance.com/api/v3/klines?symbol={symbol}&interval={interval}&limit={}",
        limit.max(1)
    );
    let payload = fetch_json(&url, "binance_spot")?;
    let rows = payload.as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for row in rows {
        let Some(values) = row.as_array() else { continue };
        if values.len() < 6 {
            continue;
        }
        out.push(json!([
            values[0],
            values[1].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[2].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[3].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[4].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            values[5].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0)
        ]));
    }
    Ok(Value::Array(out))
}

fn fetch_binance_spot_tick(symbol: &str) -> Result<Value, String> {
    let url = format!("https://api.binance.com/api/v3/ticker/bookTicker?symbol={symbol}");
    let payload = fetch_json(&url, "binance_spot")?;
    let bid = payload
        .get("bidPrice")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let ask = payload
        .get("askPrice")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    Ok(json!([{
        "timestamp_ms": Utc::now().timestamp_millis(),
        "bid": bid,
        "ask": ask,
        "last": (bid + ask) / 2.0,
    }]))
}

fn fetch_binance_spot_trade(symbol: &str, limit: usize) -> Result<Value, String> {
    let url = format!(
        "https://api.binance.com/api/v3/trades?symbol={symbol}&limit={}",
        limit.max(1)
    );
    let payload = fetch_json(&url, "binance_spot")?;
    let rows = payload.as_array().cloned().unwrap_or_default();
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "t": row.get("time").cloned().unwrap_or(Value::from(0_i64)),
                    "price": row.get("price").cloned().unwrap_or(Value::String("0".to_string())),
                    "qty": row.get("qty").cloned().unwrap_or(Value::String("0".to_string())),
                })
            })
            .collect(),
    ))
}

fn fetch_binance_spot_orderbook(symbol: &str) -> Result<Value, String> {
    let url = format!("https://api.binance.com/api/v3/depth?symbol={symbol}&limit=50");
    fetch_json(&url, "binance_spot")
}

fn fetch_bybit_kline(symbol: &str, timeframe: &str, limit: usize) -> Result<Value, String> {
    let interval = match timeframe {
        "1m" => "1",
        "5m" => "5",
        "15m" => "15",
        "30m" => "30",
        "1h" => "60",
        "4h" => "240",
        "1d" => "D",
        _ => "1",
    };
    let url = format!(
        "https://api.bybit.com/v5/market/kline?category=linear&symbol={symbol}&interval={interval}&limit={}",
        limit.max(1)
    );
    let payload = fetch_json(&url, "bybit_linear")?;
    let rows = payload
        .get("result")
        .and_then(|row| row.get("list"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let out = rows
        .into_iter()
        .filter_map(|row| row.as_array().cloned())
        .filter(|row| row.len() >= 6)
        .map(|row| {
            json!([
                row[0].as_str().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0),
                row[1].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
                row[2].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
                row[3].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
                row[4].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
                row[5].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0)
            ])
        })
        .collect::<Vec<_>>();
    Ok(Value::Array(out))
}

fn fetch_bybit_funding(symbol: &str, limit: usize) -> Result<Value, String> {
    let url = format!(
        "https://api.bybit.com/v5/market/funding/history?category=linear&symbol={symbol}&limit={}",
        limit.max(1)
    );
    let payload = fetch_json(&url, "bybit_linear")?;
    let rows = payload
        .get("result")
        .and_then(|row| row.get("list"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "fundingTime": row.get("fundingRateTimestamp").cloned().unwrap_or(Value::String("0".to_string())),
                    "fundingRate": row.get("fundingRate").cloned().unwrap_or(Value::String("0".to_string())),
                })
            })
            .collect(),
    ))
}

fn fetch_bybit_tick(symbol: &str) -> Result<Value, String> {
    let url = format!(
        "https://api.bybit.com/v5/market/tickers?category=linear&symbol={symbol}"
    );
    let payload = fetch_json(&url, "bybit_linear")?;
    let ticker = payload
        .get("result")
        .and_then(|row| row.get("list"))
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let bid = ticker
        .get("bid1Price")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let ask = ticker
        .get("ask1Price")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    Ok(json!([{
        "timestamp_ms": Utc::now().timestamp_millis(),
        "bid": bid,
        "ask": ask,
        "last": (bid + ask) / 2.0,
    }]))
}

fn fetch_bybit_trade(symbol: &str, limit: usize) -> Result<Value, String> {
    let url = format!(
        "https://api.bybit.com/v5/market/recent-trade?category=linear&symbol={symbol}&limit={}",
        limit.max(1)
    );
    let payload = fetch_json(&url, "bybit_linear")?;
    let rows = payload
        .get("result")
        .and_then(|row| row.get("list"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "t": row.get("time").cloned().unwrap_or(Value::String("0".to_string())),
                    "price": row.get("price").cloned().unwrap_or(Value::String("0".to_string())),
                    "qty": row.get("size").cloned().unwrap_or(Value::String("0".to_string())),
                })
            })
            .collect(),
    ))
}

fn fetch_bybit_orderbook(symbol: &str) -> Result<Value, String> {
    let url = format!(
        "https://api.bybit.com/v5/market/orderbook?category=linear&symbol={symbol}&limit=50"
    );
    fetch_json(&url, "bybit_linear")
}

fn fetch_kraken_tick(symbol: &str) -> Result<Value, String> {
    let url = format!("https://api.kraken.com/0/public/Ticker?pair={symbol}");
    let payload = fetch_json(&url, "kraken_spot")?;
    let result = payload
        .get("result")
        .and_then(Value::as_object)
        .and_then(|rows| rows.values().next())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let bid = result
        .get("b")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let ask = result
        .get("a")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    Ok(json!([{
        "timestamp_ms": Utc::now().timestamp_millis(),
        "bid": bid,
        "ask": ask,
        "last": (bid + ask) / 2.0,
    }]))
}

fn fetch_kraken_trade(symbol: &str, limit: usize) -> Result<Value, String> {
    let url = format!("https://api.kraken.com/0/public/Trades?pair={symbol}");
    let payload = fetch_json(&url, "kraken_spot")?;
    let rows = payload
        .get("result")
        .and_then(Value::as_object)
        .and_then(|result| {
            result
                .iter()
                .find(|(key, _)| key.as_str() != "last")
                .and_then(|(_, value)| value.as_array().cloned())
        })
        .unwrap_or_default();
    Ok(Value::Array(
        rows.into_iter()
            .take(limit.max(1))
            .filter_map(|row| row.as_array().cloned())
            .filter(|row| row.len() >= 3)
            .map(|row| {
                json!({
                    "t": row[2].as_str().and_then(|v| v.parse::<f64>().ok()).map(|v| (v * 1000.0) as i64).unwrap_or(0_i64),
                    "price": row[0].clone(),
                    "qty": row[1].clone(),
                })
            })
            .collect(),
    ))
}

fn fetch_kraken_orderbook(symbol: &str) -> Result<Value, String> {
    let url = format!("https://api.kraken.com/0/public/Depth?pair={symbol}&count=50");
    fetch_json(&url, "kraken_spot")
}

fn fetch_coinbase_tick(symbol: &str) -> Result<Value, String> {
    let url = format!("https://api.exchange.coinbase.com/products/{symbol}/ticker");
    let payload = fetch_json(&url, "coinbase_spot")?;
    let bid = payload
        .get("bid")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let ask = payload
        .get("ask")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let price = payload
        .get("price")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or((bid + ask) / 2.0);
    Ok(json!([{
        "timestamp_ms": Utc::now().timestamp_millis(),
        "bid": bid,
        "ask": ask,
        "last": price,
    }]))
}

fn fetch_coinbase_trade(symbol: &str, limit: usize) -> Result<Value, String> {
    let url = format!("https://api.exchange.coinbase.com/products/{symbol}/trades");
    let payload = fetch_json(&url, "coinbase_spot")?;
    let rows = payload.as_array().cloned().unwrap_or_default();
    Ok(Value::Array(
        rows.into_iter()
            .take(limit.max(1))
            .map(|row| {
                json!({
                    "t": row.get("time").and_then(Value::as_str).and_then(parse_iso_to_ms).unwrap_or(0_i64),
                    "price": row.get("price").cloned().unwrap_or(Value::String("0".to_string())),
                    "qty": row.get("size").cloned().unwrap_or(Value::String("0".to_string())),
                })
            })
            .collect(),
    ))
}

fn fetch_coinbase_orderbook(symbol: &str) -> Result<Value, String> {
    let url = format!("https://api.exchange.coinbase.com/products/{symbol}/book?level=2");
    fetch_json(&url, "coinbase_spot")
}

fn fetch_world_bank_macro(series_id: &str) -> Result<Value, String> {
    let indicator = if series_id.trim().is_empty() {
        "NY.GDP.MKTP.CD"
    } else {
        series_id
    };
    let url = format!(
        "https://api.worldbank.org/v2/country/WLD/indicator/{indicator}?format=json&per_page=20"
    );
    let payload = fetch_json(&url, "world_bank")?;
    let rows = payload
        .as_array()
        .and_then(|parts| parts.get(1))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                let date = row
                    .get("date")
                    .and_then(Value::as_str)
                    .unwrap_or("1970");
                json!({
                    "date": format!("{date}-01-01T00:00:00Z"),
                    "series_id": indicator,
                    "value": row.get("value").cloned().unwrap_or(Value::Null),
                })
            })
            .collect(),
    ))
}

fn fetch_ecb_macro(symbol: &str) -> Result<Value, String> {
    let quote = if symbol.trim().is_empty() { "USD" } else { symbol };
    let url = format!(
        "https://data-api.ecb.europa.eu/service/data/EXR/D.{quote}.EUR.SP00.A?format=jsondata"
    );
    let payload = fetch_json(&url, "ecb")?;
    let observations = payload
        .get("dataSets")
        .and_then(Value::as_array)
        .and_then(|sets| sets.first())
        .and_then(|set| set.get("series"))
        .and_then(Value::as_object)
        .and_then(|series| series.values().next())
        .and_then(|row| row.get("observations"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for (key, value) in observations {
        let value_num = value
            .as_array()
            .and_then(|rows| rows.first())
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        out.push(json!({
            "date": format!("{key}T00:00:00Z"),
            "series_id": format!("ECB_EXR_{quote}"),
            "value": value_num,
        }));
    }
    Ok(Value::Array(out))
}

fn fetch_ecb_tick(symbol: &str) -> Result<Value, String> {
    let quote = if symbol.trim().is_empty() { "USD" } else { symbol };
    let url = format!(
        "https://data-api.ecb.europa.eu/service/data/EXR/D.{quote}.EUR.SP00.A?format=jsondata"
    );
    let payload = fetch_json(&url, "ecb")?;
    let value_num = payload
        .get("dataSets")
        .and_then(Value::as_array)
        .and_then(|sets| sets.first())
        .and_then(|set| set.get("series"))
        .and_then(Value::as_object)
        .and_then(|series| series.values().next())
        .and_then(|row| row.get("observations"))
        .and_then(Value::as_object)
        .and_then(|obs| obs.values().last())
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    Ok(json!([{
        "timestamp_ms": Utc::now().timestamp_millis(),
        "last": value_num,
        "bid": value_num,
        "ask": value_num,
    }]))
}

fn parse_iso_to_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc).timestamp_millis())
}

fn discover_coingecko_assets(limit: usize) -> Vec<String> {
    let url = format!(
        "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page={}&page=1",
        limit.max(1)
    );
    fetch_json(&url, "coingecko")
        .ok()
        .and_then(|payload| payload.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str).map(ToString::to_string))
        .take(limit.max(1))
        .collect()
}

fn discover_frankfurter_assets(limit: usize) -> Vec<String> {
    fetch_json("https://api.frankfurter.dev/v1/currencies", "frankfurter_fx")
        .ok()
        .and_then(|payload| payload.as_object().cloned())
        .map(|rows| rows.keys().take(limit.max(1)).cloned().collect())
        .unwrap_or_default()
}

fn discover_kraken_assets(limit: usize) -> Vec<String> {
    fetch_json("https://api.kraken.com/0/public/AssetPairs", "kraken_spot")
        .ok()
        .and_then(|payload| payload.get("result").and_then(Value::as_object).cloned())
        .map(|rows| rows.keys().take(limit.max(1)).cloned().collect())
        .unwrap_or_default()
}

fn discover_coinbase_assets(limit: usize) -> Vec<String> {
    fetch_json("https://api.exchange.coinbase.com/products", "coinbase_spot")
        .ok()
        .and_then(|payload| payload.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str).map(ToString::to_string))
        .take(limit.max(1))
        .collect()
}

fn discover_binance_futures_assets(limit: usize) -> Vec<String> {
    fetch_json("https://fapi.binance.com/fapi/v1/exchangeInfo", "binance_futures")
        .ok()
        .and_then(|payload| payload.get("symbols").and_then(Value::as_array).cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| row.get("symbol").and_then(Value::as_str).map(ToString::to_string))
        .take(limit.max(1))
        .collect()
}

    fn discover_binance_spot_assets(limit: usize) -> Vec<String> {
        fetch_json("https://api.binance.com/api/v3/exchangeInfo", "binance_spot")
        .ok()
        .and_then(|payload| payload.get("symbols").and_then(Value::as_array).cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| row.get("symbol").and_then(Value::as_str).map(ToString::to_string))
        .take(limit.max(1))
        .collect()
    }

fn discover_bybit_assets(limit: usize) -> Vec<String> {
    fetch_json(
        "https://api.bybit.com/v5/market/instruments-info?category=linear",
        "bybit_linear",
    )
    .ok()
    .and_then(|payload| payload.get("result").and_then(|row| row.get("list")).and_then(Value::as_array).cloned())
    .unwrap_or_default()
    .into_iter()
    .filter_map(|row| row.get("symbol").and_then(Value::as_str).map(ToString::to_string))
    .take(limit.max(1))
    .collect()
}

fn parse_ingest_options(args: Vec<String>) -> Result<IngestOptions, Box<dyn std::error::Error>> {
    let mut options = IngestOptions {
        asset_type: "multi_asset".to_string(),
        timeframe: "1m".to_string(),
        limit: 500,
        ..IngestOptions::default()
    };
    let mut index = 0;
    while index < args.len() {
        let flag = &args[index];
        match flag.as_str() {
            "--source" => {
                options.source = next_value(&args, &mut index, flag)?.to_string();
            }
            "--symbol" => {
                options.symbol = next_value(&args, &mut index, flag)?.to_string();
            }
            "--datasets" => {
                options.datasets = next_value(&args, &mut index, flag)?
                    .split(',')
                    .filter(|dataset| !dataset.is_empty())
                    .map(ToString::to_string)
                    .collect();
            }
            "--dataset" => {
                options
                    .datasets
                    .push(next_value(&args, &mut index, flag)?.to_string());
            }
            "--asset-type" => {
                options.asset_type = next_value(&args, &mut index, flag)?.to_string();
            }
            "--timeframe" => {
                options.timeframe = next_value(&args, &mut index, flag)?.to_string();
            }
            "--limit" => {
                options.limit = next_value(&args, &mut index, flag)?.parse::<usize>()?;
            }
            "--record-root" => {
                options.record_root = Some(next_value(&args, &mut index, flag)?.to_string());
            }
            "--duckdb" => {
                options.duckdb_path = Some(next_value(&args, &mut index, flag)?.to_string());
            }
            "--manifest-root" => {
                options.manifest_root = Some(next_value(&args, &mut index, flag)?.to_string());
            }
            "--store" => {
                options.store = true;
            }
            unknown => {
                return Err(format!("unknown option: {unknown}").into());
            }
        }
        index += 1;
    }

    if options.symbol.is_empty() {
        return Err("--symbol is required".into());
    }
    if options.datasets.is_empty() {
        return Err("--datasets is required".into());
    }

    Ok(options)
}

fn next_value<'a>(
    args: &'a [String],
    index: &mut usize,
    flag: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| format!("missing value for {flag}").into())
}

fn print_json(value: &Value, include_contract: bool) -> Result<(), Box<dyn std::error::Error>> {
    if !include_contract {
        println!("{}", serde_json::to_string_pretty(value)?);
        return Ok(());
    }

    let mut object = match value {
        Value::Object(object) => object.clone(),
        _ => {
            let mut wrapper = Map::new();
            wrapper.insert("value".to_string(), value.clone());
            wrapper
        }
    };
    if include_contract {
        object.insert(
            "bridge_contract".to_string(),
            json!({
                "contract_version": BRIDGE_CONTRACT_VERSION,
                "raw_datasets": true,
                "storage_receipts": true,
                "provenance": true,
                "capabilities": true,
                "query_sources_for": true,
                "query_best_sources": true,
                "query_source_summary": true,
                "query_dataset_summary": true,
                "query_dataset_matrix": true,
                "recommend_sources": true,
                "supported_use_cases": true,
            }),
        );
    }
    println!("{}", serde_json::to_string_pretty(&Value::Object(object))?);
    Ok(())
}
