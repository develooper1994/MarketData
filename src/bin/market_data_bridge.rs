use market_data::{
    DataHub, InMemoryStorage, LocalArtifactStorage, ManifestProvenanceTracker,
    SourceAdapterRegistry, all_capabilities, best_sources_for, capability_map,
    dataset_source_matrix, dataset_summary, recommend_sources_for_use_case, source_summary,
    sources_for, supported_use_cases,
};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::env;
use std::io::{self, IsTerminal, Read};

/// Increment this any time the bridge JSON contract changes incompatibly.
const BRIDGE_CONTRACT_VERSION: &str = "1";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str);
    let command_args = if args.len() > 1 {
        args[1..].to_vec()
    } else {
        Vec::new()
    };

    if command.is_none() && try_handle_stdin_request_mode()? {
        return Ok(());
    }

    execute_command(canonical_command(command), command, command_args, None)
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
    if let Some(args) = request.get("args") {
        if let Some(array) = args.as_array() {
            return Ok(array
                .iter()
                .map(request_value_to_arg)
                .collect::<Result<Vec<_>, _>>()?);
        }
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
        "query-dataset-summary" | "qds" => {
            option_args(options, &[("dataset", "--dataset")], &[])?
        }
        "recommend-sources" | "rs" => option_args(
            options,
            &[("use_case", "--use-case"), ("limit", "--limit")],
            &[("disallow_api_key", "--disallow-api-key"), ("no_prefer_live", "--no-prefer-live")],
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
            ("record_root", "--record-root"),
            ("manifest_root", "--manifest-root"),
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

fn request_option<'a>(
    options: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Option<&'a Value> {
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
        Some("supported-use-cases") | Some("suc") => Some("supported-use-cases"),
        Some("ingest") | Some("ing") => Some("ingest"),
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

MORE EXAMPLES
  market_data_bridge doctor
  market_data_bridge query-sources-for --dataset kline --asset-class crypto_spot
  market_data_bridge query-best-sources --dataset fundamentals --include-metadata-only
  market_data_bridge query-dataset-matrix
  market_data_bridge recommend-sources --use-case crypto_backtest --limit 5
  printf '{"kline":[[1716200000000,"10","11","9","10.5","42"]]}' | \
    market_data_bridge ingest --source offline --symbol BTCUSDT --datasets kline --asset-type crypto_spot
"#
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
    store: bool,
    record_root: Option<String>,
    manifest_root: Option<String>,
}

fn ingest(
    options: IngestOptions,
    raw_input_override: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = if let Some(record_root) = &options.record_root {
        Box::new(LocalArtifactStorage::new(record_root)) as Box<dyn market_data::StorageBackend>
    } else {
        Box::new(InMemoryStorage::default()) as Box<dyn market_data::StorageBackend>
    };
    let provenance = ManifestProvenanceTracker::new(options.manifest_root.as_deref());
    let mut hub = DataHub::with_components(storage, provenance, SourceAdapterRegistry::default());
    let raw_input = if let Some(raw_input_override) = raw_input_override {
        raw_input_override
    } else {
        let mut raw_input = String::new();
        io::stdin().read_to_string(&mut raw_input)?;
        raw_input
    };
    let result = if raw_input.trim().is_empty() {
        hub.ingest_with_asset_type(
            &options.source,
            &options.symbol,
            options.datasets,
            "1m",
            500,
            options.store,
            &options.asset_type,
        )?
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
    Ok(())
}

fn parse_ingest_options(args: Vec<String>) -> Result<IngestOptions, Box<dyn std::error::Error>> {
    let mut options = IngestOptions {
        asset_type: "multi_asset".to_string(),
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
            "--record-root" => {
                options.record_root = Some(next_value(&args, &mut index, flag)?.to_string());
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

    if options.source.is_empty() {
        return Err("--source is required".into());
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
