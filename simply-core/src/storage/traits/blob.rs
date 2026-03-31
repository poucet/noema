//! BlobStore trait for content-addressable binary storage

use anyhow::Result;
use async_trait::async_trait;
use crate::storage::types::BlobHash;

/// Content-addressable blob storage trait
#[async_trait]
pub trait BlobStore: Send + Sync {
    /// Store binary data and return its SHA-256 hash
    async fn store(&self, data: &[u8]) -> Result<BlobHash>;

    /// Retrieve blob data by hash
    async fn get(&self, hash: &BlobHash) -> Result<Vec<u8>>;

    /// Check if a blob exists
    async fn exists(&self, hash: &BlobHash) -> bool;

    /// Delete a blob by hash
    ///
    /// Returns Ok(true) if deleted, Ok(false) if didn't exist
    async fn delete(&self, hash: &BlobHash) -> Result<bool>;
}
