//! Binary asset upload.

use async_trait::async_trait;
use super::types::{AssetId, BlobHash};

#[simply_rpc::rpc_service("asset")]
#[async_trait]
pub trait AssetApi: Send + Sync {
    /// Store binary content. Returns an `AssetId` for use in `InputContent::AssetRef`.
    #[rpc(base64_param = "data")]
    async fn store_asset(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<AssetId>;

    /// Get blob data by hash (for serving assets to the UI).
    #[rpc(base64_return)]
    async fn get_blob(&self, hash: &BlobHash) -> anyhow::Result<Vec<u8>>;
}
