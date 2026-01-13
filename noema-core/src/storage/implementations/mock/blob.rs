//! Mock blob store for testing

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::{traits::BlobStore, types::blob::BlobHash};

/// Mock blob store with in-memory storage
pub struct MockBlobStore {
    blobs: Mutex<HashMap<BlobHash, Vec<u8>>>,
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
    async fn store(&self, data: &[u8]) -> Result<BlobHash> {
        let mut blobs = self.blobs.lock().unwrap();
        let hash = BlobHash::hash(data);
        blobs.insert(hash.clone(), data.to_vec());

        Ok(hash)
    }

    async fn get(&self, hash: &BlobHash) -> Result<Vec<u8>> {
        let blobs = self.blobs.lock().unwrap();
        blobs
            .get(hash)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Blob not found"))
    }

    async fn exists(&self, hash: &BlobHash) -> bool {
        self.blobs.lock().unwrap().contains_key(hash)
    }

    async fn delete(&self, hash: &BlobHash) -> Result<bool> {
        Ok(self.blobs.lock().unwrap().remove(hash).is_some())
    }
}
