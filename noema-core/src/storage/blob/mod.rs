mod fs;

pub use fs::FsBlobStore;
use async_trait::async_trait;

/// Result of storing a blob
#[derive(Debug, Clone)]
pub struct StoredBlob {
    /// SHA-256 hash of the content (also serves as the blob ID)
    pub hash: String,
    /// Size in bytes
    pub size: usize,
    /// Whether this was a new blob (false if already existed)
    pub is_new: bool,
}

/// Content-addressable blob storage trait
#[async_trait]
pub trait BlobStore: Send + Sync {
    /// Store binary data and return its SHA-256 hash
    async fn store(&self, data: &[u8]) -> anyhow::Result<StoredBlob>;

    /// Retrieve blob data by hash
    async fn get(&self, hash: &str) -> anyhow::Result<Vec<u8>>;

    /// Check if a blob exists
    async fn exists(&self, hash: &str) -> bool;

    /// Delete a blob by hash
    ///
    /// Returns Ok(true) if deleted, Ok(false) if didn't exist
    async fn delete(&self, hash: &str) -> anyhow::Result<bool>;

    /// List all blob hashes in the store
    async fn list_all(&self) -> anyhow::Result<Vec<String>>;
}
