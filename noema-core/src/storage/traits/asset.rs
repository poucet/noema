//! AssetStore trait for asset metadata storage

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::AssetId;
use crate::storage::types::asset::{Asset, StoredAsset};

/// Trait for asset storage operations
///
/// Assets have a unique ID (UUID) and reference blob content via blob_hash.
/// The binary data is stored separately in a BlobStore; this trait
/// manages the metadata.
#[async_trait]
pub trait AssetStore: Send + Sync {
    /// Create a new asset record
    ///
    /// The caller is responsible for storing the actual binary data in a BlobStore
    /// and providing the blob_hash in the Asset. Returns the generated AssetId.
    async fn create_asset(&self, asset: Asset) -> Result<AssetId>;

    /// Get an asset by ID
    async fn get(&self, id: &AssetId) -> Result<Option<StoredAsset>>;

    /// Check if an asset exists
    async fn exists(&self, id: &AssetId) -> Result<bool>;

    /// Delete an asset by ID
    ///
    /// Note: This only removes the metadata. The caller should also remove
    /// the blob from the BlobStore if no other references exist.
    async fn delete(&self, id: &AssetId) -> Result<bool>;
}
