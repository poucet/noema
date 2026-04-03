//! Binary asset upload and retrieval.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use super::types::{AssetId, BlobHash};

/// Blob data with its mime type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobResponse {
    #[serde(with = "simply_rpc::base64_bytes")]
    pub data: Vec<u8>,
    pub mime_type: String,
}

#[simply_rpc::rpc_service("asset")]
#[async_trait]
pub trait AssetApi: Send + Sync {
    /// Store binary content. Returns an `AssetId` for use in `InputContent::AssetRef`.
    #[rpc(base64_param = "data", post = "/asset")]
    async fn store_asset(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<AssetId>;

    /// Get blob data + mime type by content hash.
    #[rpc(rest_get, get = "/asset/{hash}")]
    async fn get_blob(&self, hash: &BlobHash) -> anyhow::Result<BlobResponse>;
}
