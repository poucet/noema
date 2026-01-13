//! Asset types for storage

use crate::storage::ids::AssetId;

/// Asset metadata for storage (input form)
#[derive(Debug, Clone)]
pub struct Asset {
    /// SHA-256 hash of the blob content (references the blob store)
    pub blob_hash: String,

    /// MIME type of the asset (e.g., "image/png", "audio/mp3")
    pub mime_type: String,

    /// Size in bytes
    pub size_bytes: i64,

    /// Path on local filesystem (for locally-stored assets)
    pub local_path: Option<String>,

    /// Whether this asset should only be used locally (not sent to cloud models)
    pub is_private: bool,
}

impl Asset {
    /// Create a new asset with required fields
    pub fn new(blob_hash: impl Into<String>, mime_type: impl Into<String>, size_bytes: i64) -> Self {
        Self {
            blob_hash: blob_hash.into(),
            mime_type: mime_type.into(),
            size_bytes,
            local_path: None,
            is_private: false,
        }
    }

    /// Set the local path
    pub fn with_local_path(mut self, path: impl Into<String>) -> Self {
        self.local_path = Some(path.into());
        self
    }

    /// Mark as private (local-only)
    pub fn private(mut self) -> Self {
        self.is_private = true;
        self
    }
}

/// A stored asset with metadata from the database
#[derive(Debug, Clone)]
pub struct StoredAsset {
    /// Unique identifier (UUID)
    pub id: AssetId,

    /// The asset metadata
    pub asset: Asset,

    /// When this asset was created (unix timestamp ms)
    pub created_at: i64,
}

impl StoredAsset {
    /// Get the blob hash (for fetching from blob store)
    pub fn blob_hash(&self) -> &str {
        &self.asset.blob_hash
    }

    /// Get the MIME type
    pub fn mime_type(&self) -> &str {
        &self.asset.mime_type
    }

    /// Get the size in bytes
    pub fn size_bytes(&self) -> i64 {
        self.asset.size_bytes
    }

    /// Get the local path
    pub fn local_path(&self) -> Option<&str> {
        self.asset.local_path.as_deref()
    }

    /// Check if private
    pub fn is_private(&self) -> bool {
        self.asset.is_private
    }
}

