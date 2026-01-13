//! In-memory BlobStore implementation

use anyhow::Result;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::traits::BlobStore;
use crate::storage::types::StoredBlob;

/// In-memory blob store for testing
#[derive(Debug, Default)]
pub struct MemoryBlobStore {
    blobs: Mutex<HashMap<String, Vec<u8>>>,
}

impl MemoryBlobStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn compute_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }
}

#[async_trait]
impl BlobStore for MemoryBlobStore {
    async fn store(&self, data: &[u8]) -> Result<StoredBlob> {
        let hash = Self::compute_hash(data);
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
            .ok_or_else(|| anyhow::anyhow!("Blob not found: {}", hash))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_get() {
        let store = MemoryBlobStore::new();
        let data = b"hello world";

        let stored = store.store(data).await.unwrap();
        assert_eq!(stored.size, data.len());

        let retrieved = store.get(&stored.hash).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_deduplication() {
        let store = MemoryBlobStore::new();
        let data = b"duplicate";

        let first = store.store(data).await.unwrap();

        let second = store.store(data).await.unwrap();
        assert_eq!(first.hash, second.hash);
    }

    #[tokio::test]
    async fn test_exists() {
        let store = MemoryBlobStore::new();
        let data = b"test";

        assert!(!store.exists("nonexistent").await);

        let stored = store.store(data).await.unwrap();
        assert!(store.exists(&stored.hash).await);
    }

    #[tokio::test]
    async fn test_delete() {
        let store = MemoryBlobStore::new();
        let data = b"delete me";

        let stored = store.store(data).await.unwrap();
        assert!(store.exists(&stored.hash).await);

        assert!(store.delete(&stored.hash).await.unwrap());
        assert!(!store.exists(&stored.hash).await);
        assert!(!store.delete(&stored.hash).await.unwrap());
    }
}
