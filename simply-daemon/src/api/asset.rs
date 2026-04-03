//! Binary asset upload and retrieval.

use async_trait::async_trait;
use simply_rpc::BinaryResponse;
use super::types::{AssetId, BlobHash};

#[simply_rpc::rpc_service("asset")]
#[async_trait]
pub trait AssetApi: Send + Sync {
    /// Store binary content. Returns an `AssetId` for use in `InputContent::AssetRef`.
    #[rpc(post = "/asset", base64_param = "data")]
    async fn store_asset(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<AssetId>;

    /// Get blob data + mime type by content hash.
    #[rpc(get = "/asset/{hash}", immutable_cache)]
    async fn get_blob(&self, hash: &BlobHash) -> anyhow::Result<BinaryResponse>;
}
