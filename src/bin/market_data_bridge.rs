use market_data::{
    DataHub, InMemoryStorage, LocalArtifactStorage, ManifestProvenanceTracker,
    SourceAdapterRegistry,
};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::env;
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("doctor") => print_json(
            &json!({
                "status": "ok",
                "binary": "market_data_bridge",
                "crate": env!("CARGO_PKG_NAME"),
                "version": env!("CARGO_PKG_VERSION"),
                "transport": "stdin_json",
                "supported_datasets": ["kline"],
            }),
            true,
        )?,
        Some("ingest") => ingest(parse_options(args.collect())?)?,
        Some(command) => {
            return Err(format!("unknown command: {command}").into());
        }
        None => return Err("usage: market_data_bridge <doctor|ingest> [options]".into()),
    }

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

fn parse_options(args: Vec<String>) -> Result<IngestOptions, Box<dyn std::error::Error>> {
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
                "raw_datasets": true,
                "storage_receipts": true,
                "provenance": true,
            }),
        );
    }
    println!("{}", serde_json::to_string_pretty(&Value::Object(object))?);
    Ok(())
}
