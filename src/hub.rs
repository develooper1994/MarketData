use crate::contracts::{DataRequest, IngestResult};
use crate::normalize::{normalize_dataset, to_data_records};
use crate::provenance::ManifestProvenanceTracker;
use crate::quality::CanonicalDataQuality;
use crate::storage::{InMemoryStorage, StorageBackend};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt::{Display, Formatter};
use std::sync::Arc;

pub trait RawSourceAdapter: Send + Sync {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        timeframe: &str,
        limit: usize,
    ) -> HashMap<String, Value>;

    fn discover_assets(&self, _limit: usize) -> Vec<String> {
        Vec::new()
    }
}

#[derive(Default)]
pub struct SourceAdapterRegistry {
    adapters: HashMap<String, Arc<dyn RawSourceAdapter>>,
}

impl SourceAdapterRegistry {
    pub fn register(&mut self, source: impl Into<String>, adapter: Arc<dyn RawSourceAdapter>) {
        self.adapters.insert(source.into(), adapter);
    }

    pub fn get(&self, source: &str) -> Option<Arc<dyn RawSourceAdapter>> {
        self.adapters.get(source).cloned()
    }
}

#[derive(Debug)]
pub enum HubError {
    UnknownSource(String),
    Storage(std::io::Error),
}

impl Display for HubError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            HubError::UnknownSource(source) => write!(f, "Unknown source: {source}"),
            HubError::Storage(error) => write!(f, "Storage/provenance failure: {error}"),
        }
    }
}

impl std::error::Error for HubError {}

impl From<std::io::Error> for HubError {
    fn from(value: std::io::Error) -> Self {
        HubError::Storage(value)
    }
}

pub struct DataHub {
    quality: CanonicalDataQuality,
    storage: Box<dyn StorageBackend>,
    provenance: ManifestProvenanceTracker,
    adapters: SourceAdapterRegistry,
}

impl Default for DataHub {
    fn default() -> Self {
        Self {
            quality: CanonicalDataQuality,
            storage: Box::<InMemoryStorage>::default(),
            provenance: ManifestProvenanceTracker::new(None::<&str>),
            adapters: SourceAdapterRegistry::default(),
        }
    }
}

impl DataHub {
    pub fn with_components(
        storage: Box<dyn StorageBackend>,
        provenance: ManifestProvenanceTracker,
        adapters: SourceAdapterRegistry,
    ) -> Self {
        Self {
            quality: CanonicalDataQuality,
            storage,
            provenance,
            adapters,
        }
    }

    pub fn ingest(
        &mut self,
        source: &str,
        symbol: &str,
        datasets: Vec<String>,
        timeframe: &str,
        limit: usize,
        store: bool,
    ) -> Result<IngestResult, HubError> {
        let adapter = self
            .adapters
            .get(source)
            .ok_or_else(|| HubError::UnknownSource(source.to_string()))?;

        let raw = adapter.fetch_raw(symbol, &datasets, timeframe, limit);
        self.ingest_from_raw(source, symbol, datasets, raw, store)
    }

    pub fn ingest_from_raw(
        &mut self,
        source: &str,
        symbol: &str,
        datasets: Vec<String>,
        raw_datasets: HashMap<String, Value>,
        store: bool,
    ) -> Result<IngestResult, HubError> {
        self.ingest_from_raw_with_asset_type(
            source,
            symbol,
            datasets,
            raw_datasets,
            store,
            "multi_asset",
        )
    }

    pub fn ingest_from_raw_with_asset_type(
        &mut self,
        source: &str,
        symbol: &str,
        datasets: Vec<String>,
        raw_datasets: HashMap<String, Value>,
        store: bool,
        asset_type: &str,
    ) -> Result<IngestResult, HubError> {
        let mut normalized: BTreeMap<String, Vec<BTreeMap<String, Value>>> = BTreeMap::new();
        let mut records = Vec::new();
        let mut source_issues = Vec::new();
        let mut dataset_coverage = BTreeMap::new();
        let raw_datasets_for_result = raw_datasets
            .iter()
            .map(|(dataset, payload)| (dataset.clone(), payload.clone()))
            .collect();

        for dataset in &datasets {
            if let Some(raw_payload) = raw_datasets.get(dataset) {
                let items = normalize_dataset(dataset, source, symbol, raw_payload);
                dataset_coverage.insert(dataset.clone(), items.len());
                normalized.insert(dataset.clone(), items.clone());
                records.extend(to_data_records(dataset, source, asset_type, symbol, &items));
            } else {
                dataset_coverage.insert(dataset.clone(), 0);
                source_issues.push(BTreeMap::from([
                    ("source".to_string(), source.to_string()),
                    ("reason".to_string(), format!("missing_dataset:{dataset}")),
                ]));
            }
        }

        let quality_report = self.quality.validate(&records);
        let mut storage_receipts = Vec::new();
        let mut provenance = None;

        if store && !records.is_empty() {
            storage_receipts = self.storage.write(&records)?;
            let request = DataRequest {
                dataset: datasets.join(","),
                symbol: Some(symbol.to_string()),
                parameters: BTreeMap::new(),
            };
            provenance = Some(self.provenance.capture(
                request,
                source.to_string(),
                &records,
                &storage_receipts,
            )?);
        }

        Ok(IngestResult {
            source: source.to_string(),
            symbol: Some(symbol.to_string()),
            requested_datasets: datasets,
            dataset_coverage,
            raw_datasets: raw_datasets_for_result,
            normalized,
            records,
            quality_report,
            storage_receipts,
            provenance,
            source_issues,
        })
    }

    pub fn discover_assets(&self, source: &str, limit: usize) -> Result<Vec<String>, HubError> {
        let adapter = self
            .adapters
            .get(source)
            .ok_or_else(|| HubError::UnknownSource(source.to_string()))?;
        Ok(adapter.discover_assets(limit))
    }
}
