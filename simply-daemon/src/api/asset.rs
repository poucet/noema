//! Binary asset upload and retrieval.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use simply_rpc::{BinaryResponse, BinaryUpload};
use super::types::{AssetId, BlobHash};

/// Asset metadata returned by get_asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetInfo {
    pub id: AssetId,
    pub blob_hash: BlobHash,
    pub mime_type: String,
    pub size_bytes: i64,
}

#[simply_rpc::rpc_service("asset")]
#[async_trait]
pub trait AssetApi: Send + Sync {
    /// Store binary content. Returns the asset ID and blob hash.
    #[rpc(post = "/asset")]
    async fn store_asset(&self, upload: BinaryUpload) -> anyhow::Result<AssetInfo>;

    /// List all asset IDs.
    #[rpc(get = "/asset")]
    async fn list_assets(&self) -> anyhow::Result<Vec<AssetId>>;

    /// Get asset metadata by ID.
    #[rpc(get = "/asset/{id}")]
    async fn get_asset(&self, id: &AssetId) -> anyhow::Result<AssetInfo>;

    /// Get blob data + mime type by content hash.
    #[rpc(get = "/blob/{hash}", immutable_cache)]
    async fn get_blob(&self, hash: &BlobHash) -> anyhow::Result<BinaryResponse>;
}
