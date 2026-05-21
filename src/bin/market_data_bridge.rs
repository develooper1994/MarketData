use market_data::{
    DataHub, InMemoryStorage, LocalArtifactStorage, ManifestProvenanceTracker,
    SourceAdapterRegistry, all_capabilities, best_sources_for, capability_map, dataset_summary,
    recommend_sources_for_use_case, source_summary, sources_for, supported_use_cases,
};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::env;
use std::io::{self, Read};

/// Increment this any time the bridge JSON contract changes incompatibly.
const BRIDGE_CONTRACT_VERSION: &str = "1";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("help") | Some("--help") | Some("-h") => {
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
        Some("assert-contract") => assert_contract(args.collect())?,
        Some("sources") => {
            let caps = all_capabilities();
            let names: Vec<String> = caps.into_iter().map(|c| c.source).collect();
            println!("{}", serde_json::to_string_pretty(&json!(names))?);
        }
        Some("query-sources-for") => query_sources_for(args.collect())?,
        Some("query-best-sources") => query_best_sources(args.collect())?,
        Some("query-source-summary") => query_source_summary(args.collect())?,
        Some("query-dataset-summary") => query_dataset_summary(args.collect())?,
        Some("recommend-sources") => recommend_sources(args.collect())?,
        Some("supported-use-cases") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!(supported_use_cases()))?
            );
        }
        Some("ingest") => ingest(parse_ingest_options(args.collect())?)?,
        Some(command) => {
            return Err(format!("unknown command: {command}\n\n{}", help_text()).into());
        }
        None => {
            println!("{}", help_text());
        }
    }

    Ok(())
}

fn help_text() -> &'static str {
    r#"market_data_bridge — MarketData integration CLI

USAGE
  market_data_bridge <command> [options]
  market_data_bridge help
  market_data_bridge --help

COMMANDS
  doctor                Show bridge/version/contract health information
  assert-contract       Verify expected bridge contract version
  capabilities          Print full source capability registry
  sources               Print all source names
  query-sources-for     Filter sources by dataset/asset class/live support
  query-best-sources    Return ranked source recommendations for a dataset
  query-source-summary  Explain capabilities and status for one source
  query-dataset-summary Explain availability summary for one dataset
  supported-use-cases   List built-in recommendation use-cases
  recommend-sources     Recommend sources by use-case
  ingest                Normalize + quality-check + storage + provenance

EXAMPLES
  market_data_bridge doctor
  market_data_bridge query-sources-for --dataset kline --asset-class crypto_spot
  market_data_bridge query-best-sources --dataset fundamentals --include-metadata-only
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

fn ingest(options: IngestOptions) -> Result<(), Box<dyn std::error::Error>> {
    let mut raw_input = String::new();
    io::stdin().read_to_string(&mut raw_input)?;
    let raw_datasets: HashMap<String, Value> = if raw_input.trim().is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str(&raw_input)?
    };

    let storage = if let Some(record_root) = &options.record_root {
        Box::new(LocalArtifactStorage::new(record_root)) as Box<dyn market_data::StorageBackend>
    } else {
        Box::new(InMemoryStorage::default()) as Box<dyn market_data::StorageBackend>
    };
    let provenance = ManifestProvenanceTracker::new(options.manifest_root.as_deref());
    let mut hub = DataHub::with_components(storage, provenance, SourceAdapterRegistry::default());
    let result = hub.ingest_from_raw_with_asset_type(
        &options.source,
        &options.symbol,
        options.datasets,
        raw_datasets,
        options.store,
        &options.asset_type,
    )?;

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
                "recommend_sources": true,
                "supported_use_cases": true,
            }),
        );
    }
    println!("{}", serde_json::to_string_pretty(&Value::Object(object))?);
    Ok(())
}
