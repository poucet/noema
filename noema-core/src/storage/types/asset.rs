//! Asset types for storage

use crate::storage::types::BlobHash;

/// Asset metadata for storage (input form)
///
/// Use with `Stored<AssetId, Asset>` for the full stored representation.
#[derive(Debug, Clone)]
pub struct Asset {
    /// SHA-256 hash of the blob content (references the blob store)
    pub blob_hash: BlobHash,

    /// MIME type of the asset (e.g., "image/png", "audio/mp3")
    pub mime_type: String,

    /// Size in bytes
    pub size_bytes: i64,

    /// Whether this asset should only be used locally (not sent to cloud models)
    pub is_private: bool,
}

impl Asset {
    /// Create a new asset with required fields
    pub fn new(blob_hash: impl Into<BlobHash>, mime_type: impl Into<String>, size_bytes: i64) -> Self {
        Self {
            blob_hash: blob_hash.into(),
            mime_type: mime_type.into(),
            size_bytes,
            is_private: false,
        }
    }

    /// Mark as private (local-only)
    pub fn private(mut self) -> Self {
        self.is_private = true;
        self
    }
}
