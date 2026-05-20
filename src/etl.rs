use crate::contracts::IngestResult;
use crate::hub::{DataHub, HubError};

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
        self.results = self
            .symbols
            .iter()
            .map(|symbol| {
                self.hub
                    .ingest(&source, symbol, self.datasets.clone(), "1m", 500, false)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self)
    }

    pub fn results(&self) -> &[IngestResult] {
        &self.results
    }
}
