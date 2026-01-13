//! In-memory BlobStore implementation

use anyhow::Result;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::traits::BlobStore;
use crate::storage::types::BlobHash;

/// In-memory blob store for testing
#[derive(Debug, Default)]
pub struct MemoryBlobStore {
    blobs: Mutex<HashMap<BlobHash, Vec<u8>>>,
}

impl MemoryBlobStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl BlobStore for MemoryBlobStore {
    async fn store(&self, data: &[u8]) -> Result<BlobHash> {
        let hash = BlobHash::hash(data);
        let mut blobs = self.blobs.lock().unwrap();
        blobs.insert(hash.clone(), data.to_vec());

        Ok(hash)
    }

    async fn get(&self, hash: &BlobHash) -> Result<Vec<u8>> {
        let blobs = self.blobs.lock().unwrap();
        blobs
            .get(hash)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Blob not found: {}", hash.as_str()))
    }

    async fn exists(&self, hash: &BlobHash) -> bool {
        self.blobs.lock().unwrap().contains_key(hash)
    }

    async fn delete(&self, hash: &BlobHash) -> Result<bool> {
        Ok(self.blobs.lock().unwrap().remove(hash).is_some())
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
