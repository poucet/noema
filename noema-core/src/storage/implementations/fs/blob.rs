//! Content-Addressable Storage (CAS) for binary assets
//!
//! Files are stored by their SHA-256 hash, enabling:
//! - Deduplication (same content stored once)
//! - Integrity verification (hash validates content)
//! - Efficient storage (no Base64 overhead)

use crate::storage::traits::BlobStore;
use crate::storage::types::BlobHash;
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Content-addressable blob storage on filesystem
///
/// Files are stored in a sharded directory structure based on the first 2 characters
/// of their SHA-256 hash: `blob_storage/{hash[0:2]}/{hash}`
#[derive(Debug, Clone)]
pub struct FsBlobStore {
    root: PathBuf,
}

impl FsBlobStore {
    /// Create a new FsBlobStore with the given root directory
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Get the filesystem path for a blob
    pub fn path_for(&self, hash: &BlobHash) -> PathBuf {
        let hash = hash.as_str();
        if hash.len() < 2 {
            return self.root.join(hash);
        }
        let shard = &hash[0..2];
        self.root.join(shard).join(hash)
    }

    /// Get the size of a blob without reading its contents
    pub async fn size(&self, hash: &BlobHash) -> anyhow::Result<u64> {
        let path = self.path_for(hash);
        let metadata = fs::metadata(&path).await?;
        Ok(metadata.len())
    }

    /// Verify that a blob's content matches its hash
    pub async fn verify(&self, hash: &BlobHash) -> anyhow::Result<bool> {
        let data = self.get(hash).await?;
        let computed = BlobHash::from_data(&data);
        Ok(computed == *hash)
    }

    /// Clean up orphaned temp files
    pub async fn cleanup_temp_files(&self) -> anyhow::Result<usize> {
        let mut cleaned = 0;

        if !fs::try_exists(&self.root).await? {
            return Ok(0);
        }

        let mut shard_entries = fs::read_dir(&self.root).await?;
        while let Some(shard_entry) = shard_entries.next_entry().await? {
            let shard_path = shard_entry.path();

            if !shard_path.is_dir() {
                continue;
            }

            let mut blob_entries = fs::read_dir(&shard_path).await?;
            while let Some(blob_entry) = blob_entries.next_entry().await? {
                let blob_path = blob_entry.path();

                if let Some(ext) = blob_path.extension() {
                    if ext == "tmp" {
                        fs::remove_file(&blob_path).await?;
                        cleaned += 1;
                    }
                }
            }
        }

        Ok(cleaned)
    }
}

#[async_trait]
impl BlobStore for FsBlobStore {
    async fn store(&self, data: &[u8]) -> anyhow::Result<BlobHash> {
        let hash = BlobHash::from_data(data);
        let path = self.path_for(&hash);

        // Check if already exists (deduplication)
        if fs::try_exists(&path).await? {
            return Ok(hash)
        }

        // Create shard directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write atomically using a temp file
        let temp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(data).await?;
        file.sync_all().await?;
        fs::rename(&temp_path, &path).await?;

        Ok(hash)
    }

    async fn get(&self, hash: &BlobHash) -> anyhow::Result<Vec<u8>> {
        let path = self.path_for(hash);
        let data = fs::read(&path).await?;
        Ok(data)
    }

    async fn exists(&self, hash: &BlobHash) -> bool {
        self.path_for(hash).exists()
    }

    async fn delete(&self, hash: &BlobHash) -> anyhow::Result<bool> {
        let path = self.path_for(hash);
        if path.exists() {
            fs::remove_file(&path).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_blob_store() -> FsBlobStore {
        let dir = env::temp_dir().join(format!("blob_test_{}", uuid::Uuid::new_v4()));
        FsBlobStore::new(dir)
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let store = temp_blob_store();
        let data = b"Hello, World!".to_vec();

        let stored: BlobHash = store.store(&data).await.unwrap();

        let retrieved = store.get(&stored).await.unwrap();
        assert_eq!(retrieved, data);

        // Clean up
        fs::remove_dir_all(&store.root).await.ok();
    }

    #[tokio::test]
    async fn test_deduplication() {
        let store = temp_blob_store();
        let data = b"Duplicate data".to_vec();

        let first = store.store(&data).await.unwrap();  
        let second = store.store(&data).await.unwrap();
        assert_eq!(first, second);

        // Clean up
        fs::remove_dir_all(&store.root).await.ok();
    }

    #[tokio::test]
    async fn test_verify() {
        let store = temp_blob_store();
        let data = b"Verify me".to_vec();

        let stored = store.store(&data).await.unwrap();
        assert!(store.verify(&stored).await.unwrap());

        // Clean up
        fs::remove_dir_all(&store.root).await.ok();
    }

    #[tokio::test]
    async fn test_delete() {
        let store = temp_blob_store();
        let data = b"Delete me".to_vec();

        let stored = store.store(&data).await.unwrap();
        assert!(store.exists(&stored).await);

        assert!(store.delete(&stored).await.unwrap());
        assert!(!store.exists(&stored).await);
        assert!(!store.delete(&stored).await.unwrap());
        // Clean up
        fs::remove_dir_all(&store.root).await.ok();
    }
}
