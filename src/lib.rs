pub mod capabilities;
pub mod contracts;
pub mod etl;
pub mod hub;
pub mod normalize;
pub mod provenance;
pub mod quality;
pub mod query;
pub mod storage;

pub use capabilities::{
    SourceCapability, all_capabilities, canonical_dataset_name, capability_map,
};
pub use contracts::{
    DataRecord, DataRequest, IngestResult, ProvenanceRecord, QualityReport, StorageReceipt,
};
pub use etl::Etl;
pub use hub::{DataHub, HubError, RawSourceAdapter, SourceAdapterRegistry};
pub use provenance::ManifestProvenanceTracker;
pub use quality::CanonicalDataQuality;
pub use query::{
    asset_status_for_source, available_datasets, best_sources_for, dataset_status_for_source,
    dataset_summary, recommend_sources_for_use_case, source_summary, sources_for,
    supported_use_cases,
};
pub use storage::{InMemoryStorage, LocalArtifactStorage, StorageBackend};
