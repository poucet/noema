//! Blob storage types

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
