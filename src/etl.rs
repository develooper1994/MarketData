use crate::contracts::IngestResult;
use crate::hub::{DataHub, HubError};
use serde_json::Value;
use std::collections::HashMap;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

pub struct Etl {
    hub: DataHub,
    source: Option<String>,
    symbols: Vec<String>,
    datasets: Vec<String>,
    results: Vec<IngestResult>,
}

impl Etl {
    pub fn new(hub: DataHub) -> Self {
        Self {
            hub,
            source: None,
            symbols: Vec::new(),
            datasets: Vec::new(),
            results: Vec::new(),
        }
    }

    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn select_assets(mut self, symbols: Vec<String>) -> Self {
        self.symbols = symbols;
        self
    }

    pub fn fetch(mut self, datasets: Vec<String>) -> Result<Self, HubError> {
        self.datasets = datasets;
        let source = self.source.clone().ok_or_else(|| {
            HubError::UnknownSource("source must be selected before fetch".to_string())
        })?;
        // First: resolve chosen source (handles "auto")
        let actual_source = self.hub.resolve_actual_source(&source, &self.symbols.get(0).cloned().unwrap_or_default(), &self.datasets);

        // Parallelize only the network/raw fetch phase when `parallel` feature is enabled.
        #[cfg(feature = "parallel")]
        {
            let adapter = self
                .hub
                .adapter_for(&actual_source)
                .ok_or_else(|| HubError::UnknownSource(actual_source.clone()))?;

            let datasets_clone = self.datasets.clone();
            // Fetch raw payloads in parallel (adapter.fetch_raw is expected to be thread-safe)
            let raw_results: Vec<Result<(String, HashMap<String, Value>), HubError>> =
                self.symbols
                    .par_iter()
                    .map(|symbol| {
                        let sym = symbol.clone();
                        adapter
                            .fetch_raw(&sym, &datasets_clone, "1m", 500, None, false)
                            .map(|raw| (sym, raw))
                            .map_err(|e| HubError::Provider(e))
                    })
                    .collect();

            // If any fetch failed, return the first error
            let mut ordered_raws: Vec<(String, HashMap<String, Value>)> = Vec::new();
            for r in raw_results {
                match r {
                    Ok(v) => ordered_raws.push(v),
                    Err(e) => return Err(e),
                }
            }

            // Process normalization + storage sequentially (mutates hub)
            self.results = Vec::new();
            for (symbol, raw) in ordered_raws {
                let res = self.hub.ingest_from_raw(
                    &actual_source,
                    &symbol,
                    self.datasets.clone(),
                    raw,
                    false,
                    None,
                    false,
                )?;
                self.results.push(res);
            }
        }

        #[cfg(not(feature = "parallel"))]
        {
            self.results = self
                .symbols
                .iter()
                .map(|symbol| {
                    self.hub.ingest(
                        &actual_source,
                        symbol,
                        self.datasets.clone(),
                        "1m",
                        500,
                        false,
                        None,
                        false,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(self)
    }

    pub fn results(&self) -> &[IngestResult] {
        &self.results
    }
}
