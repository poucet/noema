//! Mock blob store for testing

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::storage::traits::BlobStore;
use crate::storage::types::StoredBlob;

/// Mock blob store with in-memory storage
pub struct MockBlobStore {
    blobs: Mutex<HashMap<String, Vec<u8>>>,
}

impl MockBlobStore {
    pub fn new() -> Self {
        Self {
            blobs: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MockBlobStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BlobStore for MockBlobStore {
    async fn store(&self, data: &[u8]) -> Result<StoredBlob> {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hex::encode(hasher.finalize());

        let mut blobs = self.blobs.lock().unwrap();
        blobs.insert(hash.clone(), data.to_vec());

        Ok(StoredBlob {
            hash,
            size: data.len(),
        })
    }

    async fn get(&self, hash: &str) -> Result<Vec<u8>> {
        let blobs = self.blobs.lock().unwrap();
        blobs
            .get(hash)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Blob not found"))
    }

    async fn exists(&self, hash: &str) -> bool {
        self.blobs.lock().unwrap().contains_key(hash)
    }

    async fn delete(&self, hash: &str) -> Result<bool> {
        Ok(self.blobs.lock().unwrap().remove(hash).is_some())
    }

    async fn list_all(&self) -> Result<Vec<String>> {
        Ok(self.blobs.lock().unwrap().keys().cloned().collect())
    }
}
