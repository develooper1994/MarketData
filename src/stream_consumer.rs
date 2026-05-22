use crate::hub::{DataHub, HubError};
use serde_json::Value;
use std::fs::{read_dir, create_dir_all, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Consume JSONL stream files in `streams_dir` and ingest them into `hub`.
///
/// This is a simple one-shot processor: for each `*.jsonl` file it reads all
/// lines, collects JSON items into a `tick` raw dataset and calls
/// `hub.ingest_from_raw_with_asset_type(...)`. Processed files are moved to a
/// `processed/` subdirectory to avoid re-processing.
pub fn consume_stream_files(hub: &mut DataHub, streams_dir: &str, store: bool) -> Result<usize, HubError> {
    let dir = Path::new(streams_dir);
    if !dir.exists() {
        return Ok(0);
    }

    let processed_dir = dir.join("processed");
    if let Err(e) = create_dir_all(&processed_dir) {
        return Err(HubError::Storage(std::io::Error::new(std::io::ErrorKind::Other, format!("failed to create processed dir: {}", e))));
    }

    let mut ingested_files = 0usize;

    for entry in read_dir(dir).map_err(|e| HubError::Storage(e))? {
        let entry = entry.map_err(|e| HubError::Storage(e))?;
        let path = entry.path();
        if !path.is_file() { continue; }
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if ext != "jsonl" { continue; }
        } else { continue; }

        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        // Expect pattern like tradingview_<symbol>.jsonl
        let symbol = file_name
            .trim_end_matches(".jsonl")
            .splitn(2, '_')
            .nth(1)
            .map(|s| s.to_string())
            .unwrap_or_else(|| file_name.clone());

        let file = File::open(&path).map_err(|e| HubError::Storage(e))?;
        let reader = BufReader::new(file);
        let mut items = Vec::new();
        for line_res in reader.lines() {
            if let Ok(line) = line_res {
                if line.trim().is_empty() { continue; }
                if let Ok(json_v) = serde_json::from_str::<Value>(&line) {
                    match json_v {
                        Value::Array(arr) => {
                            for it in arr { items.push(it); }
                        }
                        other => items.push(other),
                    }
                }
            }
        }

        if items.is_empty() {
            // move empty/invalid file to processed
            let target = processed_dir.join(format!("{}", file_name));
            let _ = std::fs::rename(&path, &target);
            continue;
        }

        let mut raw = std::collections::HashMap::new();
        raw.insert("tick".to_string(), Value::Array(items));

        // ingest synchronously
        let _res = hub.ingest_from_raw_with_asset_type("tradingview", &symbol, vec!["tick".to_string()], raw, store, "multi_asset")?;

        // move processed file
        let target = processed_dir.join(format!("{}", file_name));
        std::fs::rename(&path, &target).ok();
        ingested_files += 1;
    }

    Ok(ingested_files)
}
