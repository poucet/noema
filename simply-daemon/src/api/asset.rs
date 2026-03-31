//! Binary asset upload.

use async_trait::async_trait;
use super::types::{AssetId, BlobHash};

#[async_trait]
pub trait AssetApi: Send + Sync {
    /// Store binary content. Returns an `AssetId` for use in `InputContent::AssetRef`.
    async fn store_asset(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<AssetId>;

    /// Get blob data by hash (for serving assets to the UI).
    async fn get_blob(&self, hash: &BlobHash) -> anyhow::Result<Vec<u8>>;
}
