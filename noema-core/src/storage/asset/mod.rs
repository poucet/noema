//! Asset storage - content-addressed binary storage
//!
//! Assets are the binary counterpart to content blocks in the Unified Content Model.
//! This stores metadata about binary assets (images, audio, PDFs), while the actual
//! binary data is stored in a `BlobStore` (filesystem-based).
//!
//! Asset IDs are SHA-256 hashes of the binary content, enabling deduplication.

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::AssetId;

// ============================================================================
// Types
// ============================================================================

/// Asset metadata for storage (input form)
#[derive(Debug, Clone)]
pub struct Asset {
    /// MIME type of the asset (e.g., "image/png", "audio/mp3")
    pub mime_type: String,

    /// Original filename (if known)
    pub original_filename: Option<String>,

    /// Size in bytes
    pub size_bytes: i64,

    /// Path on local filesystem (for locally-stored assets)
    pub local_path: Option<String>,

    /// Whether this asset should only be used locally (not sent to cloud models)
    pub is_private: bool,
}

impl Asset {
    /// Create a new asset with required fields
    pub fn new(mime_type: impl Into<String>, size_bytes: i64) -> Self {
        Self {
            mime_type: mime_type.into(),
            original_filename: None,
            size_bytes,
            local_path: None,
            is_private: false,
        }
    }

    /// Set the original filename
    pub fn with_filename(mut self, filename: impl Into<String>) -> Self {
        self.original_filename = Some(filename.into());
        self
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
    /// Unique identifier (SHA-256 hash of content)
    pub id: AssetId,

    /// The asset metadata
    pub asset: Asset,

    /// When this asset was created (unix timestamp ms)
    pub created_at: i64,
}

impl StoredAsset {
    /// Get the MIME type
    pub fn mime_type(&self) -> &str {
        &self.asset.mime_type
    }

    /// Get the original filename
    pub fn original_filename(&self) -> Option<&str> {
        self.asset.original_filename.as_deref()
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

/// Result of storing an asset
#[derive(Clone, Debug)]
pub struct AssetStoreResult {
    /// The asset ID (SHA-256 hash)
    pub id: AssetId,

    /// Whether this was a new insertion (false = deduplicated)
    pub is_new: bool,
}

// ============================================================================
// Trait
// ============================================================================

/// Trait for asset storage operations
///
/// Assets are stored with content-addressing (SHA-256 hash as ID).
/// The binary data is stored separately in a BlobStore; this trait
/// manages the metadata.
#[async_trait]
pub trait AssetStore: Send + Sync {
    /// Store asset metadata for a blob
    ///
    /// The caller is responsible for storing the actual binary data in a BlobStore
    /// and providing the hash as the asset ID. If an asset with the same hash
    /// already exists, this returns the existing asset (deduplication).
    async fn store(&self, id: AssetId, asset: Asset) -> Result<AssetStoreResult>;

    /// Get an asset by ID (hash)
    async fn get(&self, id: &AssetId) -> Result<Option<StoredAsset>>;

    /// Check if an asset exists
    async fn exists(&self, id: &AssetId) -> Result<bool>;

    /// Delete an asset by ID
    ///
    /// Note: This only removes the metadata. The caller should also remove
    /// the blob from the BlobStore if no other references exist.
    async fn delete(&self, id: &AssetId) -> Result<bool>;
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite;
