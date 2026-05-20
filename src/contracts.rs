use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataRequest {
    pub dataset: String,
    pub symbol: Option<String>,
    pub parameters: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataRecord {
    pub key: String,
    pub observed_at: String,
    pub domain: String,
    pub source: String,
    pub asset_type: String,
    pub payload: BTreeMap<String, Value>,
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QualityReport {
    pub passed: bool,
    pub checks: Vec<String>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageReceipt {
    pub storage_id: String,
    pub location: String,
    pub record_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceRecord {
    pub request: DataRequest,
    pub source_plugin_id: String,
    pub storage_receipts: Vec<StorageReceipt>,
    pub record_keys: Vec<String>,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IngestResult {
    pub source: String,
    pub symbol: Option<String>,
    pub requested_datasets: Vec<String>,
    pub dataset_coverage: BTreeMap<String, usize>,
    pub raw_datasets: BTreeMap<String, Value>,
    pub normalized: BTreeMap<String, Vec<BTreeMap<String, Value>>>,
    pub records: Vec<DataRecord>,
    pub quality_report: QualityReport,
    pub storage_receipts: Vec<StorageReceipt>,
    pub provenance: Option<ProvenanceRecord>,
    pub source_issues: Vec<BTreeMap<String, String>>,
}
