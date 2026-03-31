//! Binary asset upload.

use async_trait::async_trait;
use crate::types::AssetId;

#[async_trait]
pub trait AssetApi: Send + Sync {
    /// Store binary content. Returns an `AssetId` for use in `InputContent::AssetRef`.
    async fn store_asset(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<AssetId>;
}
