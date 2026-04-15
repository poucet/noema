//! Binary asset upload and retrieval.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use simply_rpc::{BinaryResponse, BinaryUpload, RequestContext};
use super::types::{AssetId, BlobHash};

/// Asset metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetInfo {
    pub id: AssetId,
    pub blob_hash: BlobHash,
    pub mime_type: String,
    pub size_bytes: i64,
}

/// Full asset: metadata + binary data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub info: AssetInfo,
    #[serde(flatten)]
    pub data: BinaryResponse,
}

#[simply_rpc::rpc_service("asset")]
#[async_trait]
pub trait AssetApi: Send + Sync {
    /// Store binary content. Returns the asset ID and blob hash.
    #[rpc(post = "/asset")]
    async fn store_asset(&self, ctx: RequestContext, upload: BinaryUpload) -> anyhow::Result<AssetInfo>;

    /// List all asset IDs.
    #[rpc(get = "/asset")]
    async fn list_assets(&self, ctx: RequestContext) -> anyhow::Result<Vec<AssetId>>;

    /// Get asset metadata by ID.
    #[rpc(get = "/asset/{id}/info")]
    async fn get_asset_info(&self, ctx: RequestContext, id: &AssetId) -> anyhow::Result<AssetInfo>;

    /// Get an asset (metadata + binary data) by ID.
    #[rpc(get = "/asset/{id}", binary_response)]
    async fn get_asset(&self, ctx: RequestContext, id: &AssetId) -> anyhow::Result<BinaryResponse>;

    /// Get blob data + mime type by content hash.
    #[rpc(get = "/blob/{hash}", immutable_cache)]
    async fn get_blob(&self, ctx: RequestContext, hash: &BlobHash) -> anyhow::Result<BinaryResponse>;
}
