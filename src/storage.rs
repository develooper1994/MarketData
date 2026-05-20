use crate::contracts::{DataRecord, StorageReceipt};
use chrono::Utc;
use serde_json::to_string;
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::{Path, PathBuf};

pub trait StorageBackend: Send {
    fn write(&mut self, records: &[DataRecord]) -> std::io::Result<Vec<StorageReceipt>>;
}

#[derive(Debug, Default)]
pub struct InMemoryStorage {
    pub batches: Vec<Vec<DataRecord>>,
}

impl StorageBackend for InMemoryStorage {
    fn write(&mut self, records: &[DataRecord]) -> std::io::Result<Vec<StorageReceipt>> {
        self.batches.push(records.to_vec());
        let batch_id = self.batches.len().to_string();
        Ok(vec![StorageReceipt {
            storage_id: batch_id.clone(),
            location: format!("memory://normalized/{batch_id}"),
            record_keys: records.iter().map(|record| record.key.clone()).collect(),
        }])
    }
}

#[derive(Debug, Clone)]
pub struct LocalArtifactStorage {
    root_path: PathBuf,
}

impl LocalArtifactStorage {
    pub fn new(root_path: impl AsRef<Path>) -> Self {
        Self {
            root_path: root_path.as_ref().to_path_buf(),
        }
    }
}

impl StorageBackend for LocalArtifactStorage {
    fn write(&mut self, records: &[DataRecord]) -> std::io::Result<Vec<StorageReceipt>> {
        create_dir_all(&self.root_path)?;
        let batch_id = Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
        let target = self.root_path.join(format!("ingestion-{batch_id}.jsonl"));
        let mut file = File::create(&target)?;
        for record in records {
            file.write_all(to_string(record)?.as_bytes())?;
            file.write_all(b"\n")?;
        }

        Ok(vec![StorageReceipt {
            storage_id: batch_id,
            location: target.to_string_lossy().to_string(),
            record_keys: records.iter().map(|record| record.key.clone()).collect(),
        }])
    }
}
