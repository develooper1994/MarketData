pub mod contracts;
pub mod etl;
pub mod hub;
pub mod normalize;
pub mod provenance;
pub mod quality;
pub mod storage;

pub use contracts::{
    DataRecord, DataRequest, IngestResult, ProvenanceRecord, QualityReport, StorageReceipt,
};
pub use etl::Etl;
pub use hub::{DataHub, HubError, RawSourceAdapter, SourceAdapterRegistry};
pub use provenance::ManifestProvenanceTracker;
pub use quality::CanonicalDataQuality;
pub use storage::{InMemoryStorage, LocalArtifactStorage, StorageBackend};
