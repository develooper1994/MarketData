pub mod candidates;
pub mod capabilities;
pub mod contracts;
pub mod etl;
pub mod heuristics;
pub mod hub;
pub mod matcher;
pub mod normalize;
pub mod provenance;
pub mod providers;
pub mod quality;
pub mod query;
pub mod source_health;
pub mod source_registry;
pub mod source_selector;
pub mod storage;
pub mod stream_consumer;
pub mod streaming;

pub use crate::source_registry::{SourceMetadata, SourceRegistry};
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
    asset_status_for_source, available_datasets, best_sources_for, dataset_source_matrix,
    dataset_status_for_source, dataset_summary, recommend_sources_for_use_case, source_summary,
    sources_for, supported_use_cases,
};
pub use storage::{InMemoryStorage, LocalArtifactStorage, StorageBackend};

/// Convenience re-exports for library consumers.
///
/// Import everything you need for typical usage with:
///
/// ```rust,no_run
/// use market_data::prelude::*;
/// ```
pub mod prelude {
    pub use crate::capabilities::{
        SourceCapability, all_capabilities, canonical_dataset_name, capability_map,
    };
    pub use crate::contracts::{
        DataRecord, DataRequest, IngestResult, ProvenanceRecord, QualityReport, StorageReceipt,
    };
    pub use crate::etl::Etl;
    pub use crate::hub::{DataHub, HubError, RawSourceAdapter, SourceAdapterRegistry};
    pub use crate::provenance::ManifestProvenanceTracker;
    pub use crate::query::{
        best_sources_for, dataset_source_matrix, dataset_summary, recommend_sources_for_use_case,
        source_summary, sources_for, supported_use_cases,
    };
    pub use crate::storage::{InMemoryStorage, LocalArtifactStorage, StorageBackend};
}
