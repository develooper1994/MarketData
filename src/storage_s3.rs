#![cfg(feature = "s3")]

use crate::contracts::{DataRecord, StorageReceipt};
use chrono::Utc;
use serde_json::to_string;
use std::io::{Error, ErrorKind, Result};

pub struct S3Storage {
    bucket: String,
}

impl S3Storage {
    pub fn new(bucket: impl Into<String>) -> Self {
        Self { bucket: bucket.into() }
    }
}

impl crate::storage::StorageBackend for S3Storage {
    fn write(&mut self, records: &[DataRecord]) -> Result<Vec<StorageReceipt>> {
        let batch_id = Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
        let key = format!("ingestion-{}.jsonl", batch_id);

        let mut buf: Vec<u8> = Vec::new();
        for record in records {
            let s = to_string(record).map_err(|e| Error::new(ErrorKind::Other, format!("serde error: {}", e)))?;
            buf.extend_from_slice(s.as_bytes());
            buf.push(b'\n');
        }

        // Run async AWS client in a local tokio runtime to keep the public API blocking
        let bucket = self.bucket.clone();
        let key_clone = key.clone();
        let result = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt.block_on(async move {
                let config = aws_config::from_env().load().await;
                let client = aws_sdk_s3::Client::new(&config);
                client
                    .put_object()
                    .bucket(bucket)
                    .key(key_clone)
                    .body(aws_sdk_s3::types::ByteStream::from(buf))
                    .send()
                    .await
            }),
            Err(e) => return Err(Error::new(ErrorKind::Other, format!("tokio runtime error: {}", e))),
        };

        match result {
            Ok(_) => Ok(vec![StorageReceipt {
                storage_id: batch_id,
                location: format!("s3://{}/{}", self.bucket, key),
                record_keys: records.iter().map(|r| r.key.clone()).collect(),
            }]),
            Err(e) => Err(Error::new(ErrorKind::Other, format!("s3 put error: {}", e))),
        }
    }
}
