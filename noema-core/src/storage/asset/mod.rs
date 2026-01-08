//! Asset storage trait and implementations
//!
//! Provides the `AssetStore` trait for managing asset metadata.
//! This stores metadata about assets (like mime type, filename, etc.),
//! while the actual binary data is stored in a `BlobStore`.

use anyhow::Result;
use async_trait::async_trait;

// ============================================================================
// Types
// ============================================================================

/// Asset metadata stored in the database
#[derive(Debug, Clone)]
pub struct AssetInfo {
    pub id: String,
    pub mime_type: String,
    pub original_filename: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub local_path: Option<String>,
}

// ============================================================================
// Trait
// ============================================================================

/// Trait for asset metadata storage operations
#[async_trait]
pub trait AssetStore: Send + Sync {
    /// Register an asset in the database
    async fn register_asset(
        &self,
        hash: &str,
        mime_type: &str,
        original_filename: Option<&str>,
        file_size_bytes: Option<i64>,
        local_path: Option<&str>,
    ) -> Result<()>;

    /// Get asset info by hash
    async fn get_asset(&self, hash: &str) -> Result<Option<AssetInfo>>;
}

#[cfg(feature = "sqlite")]
pub (crate) mod sqlite;
