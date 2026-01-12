//! AssetStore trait for asset metadata storage

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::types::asset::{Asset, AssetStoreResult, StoredAsset};
use crate::storage::ids::AssetId;

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
