#![cfg(feature = "s3")]

use crate::contracts::DataRecord;
use crate::storage::StorageReceipt;
use std::io::{Error, ErrorKind, Result};

pub struct S3Storage {
    // configuration placeholder
    bucket: String,
}

impl S3Storage {
    pub fn new(bucket: impl Into<String>) -> Self {
        Self { bucket: bucket.into() }
    }
}

impl crate::storage::StorageBackend for S3Storage {
    fn write(&mut self, _records: &[DataRecord]) -> Result<Vec<StorageReceipt>> {
        Err(Error::new(
            ErrorKind::Other,
            "S3Storage backend is a placeholder and requires implementation",
        ))
    }
}
