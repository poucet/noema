//! Content-Addressable Storage (CAS) for binary assets
//!
//! Files are stored by their SHA-256 hash, enabling:
//! - Deduplication (same content stored once)
//! - Integrity verification (hash validates content)
//! - Efficient storage (no Base64 overhead)

use crate::storage::traits::BlobStore;
use crate::storage::types::StoredBlob;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
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
    pub fn path_for(&self, hash: &str) -> PathBuf {
        if hash.len() < 2 {
            return self.root.join(hash);
        }
        let shard = &hash[0..2];
        self.root.join(shard).join(hash)
    }

    /// Compute SHA-256 hash of data
    pub fn compute_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        hex::encode(result)
    }

    /// Get the size of a blob without reading its contents
    pub async fn size(&self, hash: &str) -> anyhow::Result<u64> {
        let path = self.path_for(hash);
        let metadata = fs::metadata(&path).await?;
        Ok(metadata.len())
    }

    /// Verify that a blob's content matches its hash
    pub async fn verify(&self, hash: &str) -> anyhow::Result<bool> {
        let data = self.get(hash).await?;
        let computed = Self::compute_hash(&data);
        Ok(computed == hash)
    }

    /// Get total size of all blobs in bytes
    pub async fn total_size(&self) -> anyhow::Result<u64> {
        let mut total = 0u64;

        for hash in self.list_all().await? {
            total += self.size(&hash).await?;
        }

        Ok(total)
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
    async fn store(&self, data: &[u8]) -> anyhow::Result<StoredBlob> {
        let hash = Self::compute_hash(data);
        let path = self.path_for(&hash);

        // Check if already exists (deduplication)
        if fs::try_exists(&path).await? {
            return Ok(StoredBlob {
                hash,
                size: data.len(),
            });
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

        Ok(StoredBlob {
            hash,
            size: data.len(),
        })
    }

    async fn get(&self, hash: &str) -> anyhow::Result<Vec<u8>> {
        let path = self.path_for(hash);
        let data = fs::read(&path).await?;
        Ok(data)
    }

    async fn exists(&self, hash: &str) -> bool {
        self.path_for(hash).exists()
    }

    async fn delete(&self, hash: &str) -> anyhow::Result<bool> {
        let path = self.path_for(hash);
        if path.exists() {
            fs::remove_file(&path).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn list_all(&self) -> anyhow::Result<Vec<String>> {
        let mut hashes = Vec::new();

        if !fs::try_exists(&self.root).await? {
            return Ok(hashes);
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

                if blob_path.is_file() {
                    if let Some(filename) = blob_path.file_name() {
                        if let Some(hash) = filename.to_str() {
                            // Skip temp files
                            if !hash.ends_with(".tmp") {
                                hashes.push(hash.to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok(hashes)
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

        let stored: StoredBlob = store.store(&data).await.unwrap();
        assert_eq!(stored.size, data.len());

        let retrieved = store.get(&stored.hash).await.unwrap();
        assert_eq!(retrieved, data);

        // Clean up
        fs::remove_dir_all(&store.root).await.ok();
    }

    #[tokio::test]
    async fn test_deduplication() {
        let store = temp_blob_store();
        let data = b"Duplicate data".to_vec();

        let first = store.store(&data).await.unwrap();
        assert_eq!(first.size, data.len());

        let second = store.store(&data).await.unwrap();
        assert_eq!(second.size, data.len());
        assert_eq!(first.hash, second.hash);

        // Clean up
        fs::remove_dir_all(&store.root).await.ok();
    }

    #[test]
    fn test_hash_computation() {
        let data = b"test";
        let hash = FsBlobStore::compute_hash(data);
        // Known SHA-256 hash of "test"
        assert_eq!(
            hash,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }

    #[tokio::test]
    async fn test_verify() {
        let store = temp_blob_store();
        let data = b"Verify me".to_vec();

        let stored = store.store(&data).await.unwrap();
        assert!(store.verify(&stored.hash).await.unwrap());

        // Clean up
        fs::remove_dir_all(&store.root).await.ok();
    }

    #[tokio::test]
    async fn test_delete() {
        let store = temp_blob_store();
        let data = b"Delete me".to_vec();

        let stored = store.store(&data).await.unwrap();
        assert!(store.exists(&stored.hash).await);

        assert!(store.delete(&stored.hash).await.unwrap());
        assert!(!store.exists(&stored.hash).await);
        assert!(!store.delete(&stored.hash).await.unwrap());

        // Clean up
        fs::remove_dir_all(&store.root).await.ok();
    }
}
