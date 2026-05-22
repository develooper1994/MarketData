use crate::capabilities::canonical_dataset_name;
use crate::contracts::{DataRequest, IngestResult};
use crate::normalize::{normalize_dataset, to_data_records};
use crate::provenance::ManifestProvenanceTracker;
use crate::quality::CanonicalDataQuality;
use crate::storage::{InMemoryStorage, StorageBackend};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use crate::streaming::StreamingAdapterRegistry;

pub trait RawSourceAdapter: Send + Sync {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        timeframe: &str,
        limit: usize,
    ) -> Result<HashMap<String, Value>, crate::providers::errors::ProviderError>;

    fn discover_assets(&self, _limit: usize) -> Vec<String> {
        Vec::new()
    }
}

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

impl Default for SourceAdapterRegistry {
    fn default() -> Self {
        let mut registry = Self {
            adapters: HashMap::new(),
        };
        registry.register("offline", Arc::new(OfflineReferenceAdapter));
        registry.register("offline_fallback", Arc::new(OfflineReferenceAdapter));
        registry
    }
}

struct OfflineReferenceAdapter;

impl RawSourceAdapter for OfflineReferenceAdapter {
    fn fetch_raw(
        &self,
        _symbol: &str,
        datasets: &[String],
        _timeframe: &str,
        _limit: usize,
    ) -> Result<HashMap<String, Value>, crate::providers::errors::ProviderError> {
        // Intentionally deterministic reference payloads for offline smoke tests
        // and bridge compatibility checks. This is a safe fallback adapter, not
        // a production live-provider implementation.
        Ok(datasets
            .iter()
            .map(|dataset| {
                let canonical = canonical_dataset_name(dataset);
                let payload = match canonical {
                    "kline" => json!([[1716200000000_i64, "10", "11", "9", "10.5", "42"]]),
                    "tick" => {
                        json!([{ "timestamp_ms": 1716200000000_i64, "bid": "10.5", "ask": "10.6", "last": "10.55" }])
                    }
                    "trade" => {
                        json!([{ "t": 1716200000000_i64, "price": "10.5", "qty": "0.5", "side": "buy" }])
                    }
                    "orderbook" => {
                        json!({ "timestamp_ms": 1716200000000_i64, "bids": [["10.0", "1"]], "asks": [["10.1", "1"]] })
                    }
                    "funding" => json!([{ "fundingTime": 1716200000000_i64, "fundingRate": "0.0001" }]),
                    "news" => {
                        json!([{ "publishedAt": "2024-01-01T00:00:00Z", "title": "Offline news sample", "url": "https://example.com/offline-news" }])
                    }
                    "macro" => json!([{ "date": "2024-01-01T00:00:00Z", "value": "5.5", "series_id": "OFFLINE_MACRO" }]),
                    "fundamentals" => {
                        json!([{ "date": "2024-01-01T00:00:00Z", "revenue": 100000000, "eps": "1.25" }])
                    }
                    "corporate_actions" => {
                        json!([{ "date": "2024-01-01T00:00:00Z", "type": "dividend", "amount": "0.25" }])
                    }
                    _ => Value::Array(Vec::new()),
                };
                (canonical.to_string(), payload)
            })
            .collect())
    }

    fn discover_assets(&self, limit: usize) -> Vec<String> {
        let assets = vec![
            "BTCUSDT".to_string(),
            "ETHUSDT".to_string(),
            "AAPL".to_string(),
            "MSFT".to_string(),
        ];
        assets.into_iter().take(limit).collect()
    }
}

#[derive(Debug)]
pub enum HubError {
    UnknownSource(String),
    Storage(std::io::Error),
    Provider(crate::providers::errors::ProviderError),
}

impl Display for HubError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            HubError::UnknownSource(source) => write!(f, "Unknown source: {source}"),
            HubError::Storage(error) => write!(f, "Storage/provenance failure: {error}"),
            HubError::Provider(err) => write!(f, "Provider error: {err}"),
        }
    }
}

impl std::error::Error for HubError {}

impl From<std::io::Error> for HubError {
    fn from(value: std::io::Error) -> Self {
        HubError::Storage(value)
    }
}

impl From<crate::providers::errors::ProviderError> for HubError {
    fn from(err: crate::providers::errors::ProviderError) -> Self {
        HubError::Provider(err)
    }
}

pub struct DataHub {
    quality: CanonicalDataQuality,
    storage: Box<dyn StorageBackend>,
    provenance: ManifestProvenanceTracker,
    adapters: SourceAdapterRegistry,
    streaming: StreamingAdapterRegistry,
}

impl Default for DataHub {
    fn default() -> Self {
        Self {
            quality: CanonicalDataQuality,
            storage: Box::<InMemoryStorage>::default(),
            provenance: ManifestProvenanceTracker::new(None::<&str>),
            adapters: SourceAdapterRegistry::default(),
            streaming: StreamingAdapterRegistry::default(),
        }
    }
}

impl DataHub {
    pub fn with_components(
        storage: Box<dyn StorageBackend>,
        provenance: ManifestProvenanceTracker,
        adapters: SourceAdapterRegistry,
        streaming: StreamingAdapterRegistry,
    ) -> Self {
        Self {
            quality: CanonicalDataQuality,
            storage,
            provenance,
            adapters,
            streaming,
        }
    }

    /// Start a background streaming session for a source/symbol.
    pub fn start_stream(&mut self, source: &str, symbol: &str, datasets: Vec<String>) -> Result<(), HubError> {
        let adapter = self
            .streaming
            .get(source)
            .ok_or_else(|| HubError::UnknownSource(source.to_string()))?;
        adapter
            .start_stream(symbol, &datasets)
            .map_err(|e| HubError::Provider(e))
    }

    /// Stop a background streaming session.
    pub fn stop_stream(&mut self, source: &str, symbol: &str) -> Result<(), HubError> {
        let adapter = self
            .streaming
            .get(source)
            .ok_or_else(|| HubError::UnknownSource(source.to_string()))?;
        adapter
            .stop_stream(symbol)
            .map_err(|e| HubError::Provider(e))
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

        let raw = adapter.fetch_raw(symbol, &datasets, timeframe, limit)?;
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
        let canonical_datasets = canonicalize_requested_datasets(&datasets);
        let raw_datasets_for_result = raw_datasets
            .iter()
            .map(|(dataset, payload)| (dataset.clone(), payload.clone()))
            .collect();

        for dataset in &canonical_datasets {
            if let Some(raw_payload) = select_raw_dataset_payload(&raw_datasets, dataset) {
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
            requested_datasets: canonical_datasets,
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

fn canonicalize_requested_datasets(datasets: &[String]) -> Vec<String> {
    let mut canonical = Vec::new();
    for dataset in datasets {
        let resolved = canonical_dataset_name(dataset).to_string();
        if !canonical.iter().any(|item| item == &resolved) {
            canonical.push(resolved);
        }
    }
    canonical
}

fn select_raw_dataset_payload<'a>(
    raw_datasets: &'a HashMap<String, Value>,
    canonical_dataset: &str,
) -> Option<&'a Value> {
    raw_datasets.get(canonical_dataset).or_else(|| {
        raw_datasets.iter().find_map(|(dataset_name, payload)| {
            (canonical_dataset_name(dataset_name) == canonical_dataset).then_some(payload)
        })
    })
}
