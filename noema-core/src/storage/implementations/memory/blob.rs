//! In-memory BlobStore implementation

use anyhow::Result;
use async_trait::async_trait;
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
    use std::str::FromStr;

    use super::*;

    #[tokio::test]
    async fn test_store_and_get() {
        let store = MemoryBlobStore::new();
        let data = b"hello world";

        let blob_hash: BlobHash = store.store(data).await.unwrap();

        let retrieved = store.get(&blob_hash).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_deduplication() {
        let store = MemoryBlobStore::new();
        let data = b"duplicate";

        let first = store.store(data).await.unwrap();

        let second = store.store(data).await.unwrap();
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn test_exists() {
        let store = MemoryBlobStore::new();
        let data = b"test";

        let invalid_hash = FromStr::from_str("nonexistent").unwrap();

        assert!(!store.exists(&invalid_hash).await);

        let blob_hash = store.store(data).await.unwrap();
        assert!(store.exists(&blob_hash).await);
    }

    #[tokio::test]
    async fn test_delete() {
        let store = MemoryBlobStore::new();
        let data = b"delete me";

        let blob_hash = store.store(data).await.unwrap();
        assert!(store.exists(&blob_hash).await);

        assert!(store.delete(&blob_hash).await.unwrap());
        assert!(!store.exists(&blob_hash).await);
        assert!(!store.delete(&blob_hash).await.unwrap());
    }
}
