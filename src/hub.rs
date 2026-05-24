use crate::capabilities::canonical_dataset_name;
use crate::capabilities::capability_map;
use crate::contracts::{DataRequest, IngestResult};
use crate::normalize::{normalize_dataset, to_data_records};
use crate::provenance::ManifestProvenanceTracker;
use crate::quality::CanonicalDataQuality;
use crate::source_registry::SourceRegistry;
use crate::source_selector::SourceSelector;
use crate::storage::{InMemoryStorage, StorageBackend};
use crate::streaming::StreamingAdapterRegistry;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
// Note: `SourceHealth` and `Duration` are used elsewhere; avoid importing
// them here to prevent unused-import lints when building all features.
#[cfg(feature = "metrics")]
use metrics::{counter, histogram};

pub trait RawSourceAdapter: Send + Sync {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        timeframe: &str,
        limit: usize,
        requested_asset_class: Option<&str>,
        force_asset_class: bool,
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
        // Wrap adapters with a small enforcing wrapper that knows the source id.
        // This allows adapter-level optional enforcement of `requested_asset_class`
        // when `force_asset_class` is set. Hub-level enforcement remains the
        // primary gate, but this wrapper provides defense-in-depth for direct
        // adapter calls and keeps provider implementations simple.
        let source_str = source.into();
        let wrapper = AdapterWrapper {
            source: source_str.clone(),
            inner: adapter,
        };
        self.adapters.insert(source_str, Arc::new(wrapper));
    }

    pub fn get(&self, source: &str) -> Option<Arc<dyn RawSourceAdapter>> {
        self.adapters.get(source).cloned()
    }
}

struct AdapterWrapper {
    source: String,
    inner: Arc<dyn RawSourceAdapter>,
}

impl RawSourceAdapter for AdapterWrapper {
    fn fetch_raw(
        &self,
        symbol: &str,
        datasets: &[String],
        timeframe: &str,
        limit: usize,
        requested_asset_class: Option<&str>,
        force_asset_class: bool,
    ) -> Result<HashMap<String, Value>, crate::providers::errors::ProviderError> {
        // If the caller requested to force an asset class, and the provider
        // does not advertise support for that class, return an empty payload
        // so upstream logic can report an unsupported-class issue. This is a
        // lightweight, per-adapter check using the capability map.
        if let (Some(req_ac), true) = (requested_asset_class, force_asset_class) {
            let caps = crate::capabilities::capability_map();
            if let Some(cap) = caps.get(self.source.as_str()) {
                let supports = cap.asset_classes.iter().any(|ac| ac == req_ac);
                if !supports {
                    return Ok(HashMap::new());
                }
            }
        }

        self.inner.fetch_raw(
            symbol,
            datasets,
            timeframe,
            limit,
            requested_asset_class,
            force_asset_class,
        )
    }

    fn discover_assets(&self, limit: usize) -> Vec<String> {
        self.inner.discover_assets(limit)
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
        _requested_asset_class: Option<&str>,
        _force_asset_class: bool,
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

    /// Return a clone of the adapter for a given source, if present.
    pub fn adapter_for(&self, source: &str) -> Option<Arc<dyn RawSourceAdapter>> {
        self.adapters.get(source).map(|a| a.clone())
    }

    /// Resolve the actual source id to use when the caller provided `auto`.
    /// This mirrors the resolution logic used by `ingest` but is provided
    /// as a read-only helper so callers can perform network fetches in
    /// parallel without taking a mutable borrow on the hub.
    pub fn resolve_actual_source(&self, requested: &str, symbol: &str, datasets: &Vec<String>) -> String {
        if requested == "auto" || requested.is_empty() {
            let caps = capability_map();
            let primary_dataset = datasets
                .first()
                .cloned()
                .unwrap_or_else(|| "kline".to_string());
            let registry_path = env::var("SOURCE_METADATA_PATH").unwrap_or_else(|_| {
                format!("{}/config/source_metadata.yaml", env!("CARGO_MANIFEST_DIR"))
            });
            let registry = SourceRegistry::load_from_path(&registry_path).unwrap_or_default();
            let selector = SourceSelector::select(
                symbol,
                &primary_dataset,
                &registry,
                None,
                None,
                false,
                false,
            );

            if let Some(chosen_reg) = selector.chosen {
                let mut best_match: Option<String> = None;
                for (key, _) in caps.into_iter() {
                    if key == chosen_reg || key.contains(&chosen_reg) {
                        best_match = Some(key);
                        break;
                    }
                }
                best_match.unwrap_or_else(|| "offline_fallback".to_string())
            } else {
                "offline_fallback".to_string()
            }
        } else {
            requested.to_string()
        }
    }

    /// Start a background streaming session for a source/symbol.
    pub fn start_stream(
        &mut self,
        source: &str,
        symbol: &str,
        datasets: Vec<String>,
    ) -> Result<(), HubError> {
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
        requested_asset_class: Option<&str>,
        force_asset_class: bool,
    ) -> Result<IngestResult, HubError> {
        // If caller requested automatic selection, consult the registry + selector
        #[cfg(feature = "metrics")]
        let ingest_start = std::time::Instant::now();
        let actual_source = if source == "auto" || source.is_empty() {
            let caps = capability_map();
            let primary_dataset = datasets
                .first()
                .cloned()
                .unwrap_or_else(|| "kline".to_string());
            let registry_path = env::var("SOURCE_METADATA_PATH").unwrap_or_else(|_| {
                format!("{}/config/source_metadata.yaml", env!("CARGO_MANIFEST_DIR"))
            });
            let registry = SourceRegistry::load_from_path(&registry_path).unwrap_or_default();
            let selector = SourceSelector::select(
                symbol,
                &primary_dataset,
                &registry,
                None,
                None,
                false,
                false,
            );

            // prefer mapped capability keys that correspond to registry id
            if let Some(chosen_reg) = selector.chosen {
                let mut best_match: Option<String> = None;
                for (key, _) in caps.into_iter() {
                    if key == chosen_reg || key.contains(&chosen_reg) {
                        best_match = Some(key);
                        break;
                    }
                }
                best_match.unwrap_or_else(|| "offline_fallback".to_string())
            } else {
                "offline_fallback".to_string()
            }
        } else {
            source.to_string()
        };

        let adapter = self
            .adapters
            .get(actual_source.as_str())
            .ok_or_else(|| HubError::UnknownSource(actual_source.clone()))?;

        // If the caller requested a forced asset class, ensure the chosen source
        // actually supports that asset class according to the capability map.
        if let (Some(req_ac), true) = (requested_asset_class, force_asset_class) {
            let caps = capability_map();
            if let Some(cap) = caps.get(actual_source.as_str()) {
                let supports = cap.asset_classes.iter().any(|ac| ac == req_ac);
                if !supports {
                    // Short-circuit: produce an empty raw payload and a result that
                    // contains a clear source issue indicating unsupported class.
                    let empty_raw: HashMap<String, Value> = HashMap::new();
                    let mut result = self.ingest_from_raw(
                        actual_source.as_str(),
                        symbol,
                        datasets,
                        empty_raw,
                        store,
                        requested_asset_class,
                        force_asset_class,
                    )?;
                    result.source_issues.push(BTreeMap::from([
                        ("source".to_string(), actual_source.clone()),
                        (
                            "reason".to_string(),
                            format!("unsupported_asset_class:{}", req_ac),
                        ),
                    ]));
                    return Ok(result);
                }
            }
        }

        let raw = adapter.fetch_raw(
            symbol,
            &datasets,
            timeframe,
            limit,
            requested_asset_class,
            force_asset_class,
        )?;
        self.ingest_from_raw(
            actual_source.as_str(),
            symbol,
            datasets,
            raw,
            store,
            requested_asset_class,
            force_asset_class,
        )
        .map(|res| {
            #[cfg(feature = "metrics")]
            {
                let elapsed = ingest_start.elapsed().as_secs_f64();
                counter!("marketdata.ingest.count", 1, "source" => actual_source.clone());
                histogram!("marketdata.ingest.latency_seconds", elapsed, "source" => actual_source);
            }
            res
        })
    }

    pub fn ingest_from_raw(
        &mut self,
        source: &str,
        symbol: &str,
        datasets: Vec<String>,
        raw_datasets: HashMap<String, Value>,
        store: bool,
        requested_asset_class: Option<&str>,
        force_asset_class: bool,
    ) -> Result<IngestResult, HubError> {
        self.ingest_from_raw_with_asset_type(
            source,
            symbol,
            datasets,
            raw_datasets,
            store,
            "multi_asset",
            requested_asset_class,
            force_asset_class,
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
        requested_asset_class: Option<&str>,
        force_asset_class: bool,
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
            let mut params: BTreeMap<String, Value> = BTreeMap::new();
            if let Some(ac) = requested_asset_class {
                params.insert(
                    "requested_asset_class".to_string(),
                    Value::String(ac.to_string()),
                );
            }
            params.insert(
                "force_asset_class".to_string(),
                Value::Bool(force_asset_class),
            );
            params.insert(
                "asset_type".to_string(),
                Value::String(asset_type.to_string()),
            );

            let request = DataRequest {
                dataset: datasets.join(","),
                symbol: Some(symbol.to_string()),
                parameters: params,
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
