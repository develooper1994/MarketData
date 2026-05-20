use crate::contracts::{DataRecord, DataRequest, ProvenanceRecord, StorageReceipt};
use serde_json::to_vec_pretty;
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct ManifestProvenanceTracker {
    manifest_root: Option<PathBuf>,
    entries: Vec<ProvenanceRecord>,
}

impl ManifestProvenanceTracker {
    pub fn new(manifest_root: Option<impl AsRef<Path>>) -> Self {
        Self {
            manifest_root: manifest_root.map(|path| path.as_ref().to_path_buf()),
            entries: Vec::new(),
        }
    }

    pub fn capture(
        &mut self,
        request: DataRequest,
        source_plugin_id: String,
        records: &[DataRecord],
        storage_receipts: &[StorageReceipt],
    ) -> std::io::Result<ProvenanceRecord> {
        let entry = ProvenanceRecord {
            request,
            source_plugin_id,
            storage_receipts: storage_receipts.to_vec(),
            record_keys: records.iter().map(|record| record.key.clone()).collect(),
            revision: format!("manifest-{}", self.entries.len() + 1),
        };

        if let Some(root) = &self.manifest_root {
            create_dir_all(root)?;
            let target = root.join(format!("{}.json", entry.revision));
            let mut file = File::create(target)?;
            file.write_all(&to_vec_pretty(&entry)?)?;
        }

        self.entries.push(entry.clone());
        Ok(entry)
    }

    pub fn entries(&self) -> &[ProvenanceRecord] {
        &self.entries
    }
}
