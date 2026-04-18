//! Semantic search API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use simply_rpc::RequestContext;
#[cfg(feature = "ts")]
use ts_rs::TS;

use crate::types::EmbeddingQueueStatus;

/// A search result with document context.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "../admin/src/lib/generated/types/"))]
pub struct SearchHit {
    pub document_id: String,
    pub document_title: String,
    pub document_type: String,
    pub tab_id: String,
    pub chunk_text: String,
    pub chunk_index: u32,
    pub score: f32,
}

/// Search request.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "../admin/src/lib/generated/types/"))]
pub struct SearchRequest {
    pub query: String,
    pub document_type: Option<String>,
    pub top_k: Option<usize>,
}

/// Status of a reindex operation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "../admin/src/lib/generated/types/"))]
pub struct ReindexStatus {
    pub message: String,
    pub tabs_queued: usize,
}

#[simply_rpc::rpc_service("search")]
#[async_trait]
pub trait SearchApi: Send + Sync {
    /// Semantic search over embedded documents.
    #[rpc(post = "/search")]
    async fn search(&self, ctx: &RequestContext, request: SearchRequest) -> anyhow::Result<Vec<SearchHit>>;

    /// Re-embed all documents. Runs in the background; returns immediately.
    #[rpc(post = "/search/reindex", no_tool)]
    async fn reindex(&self, ctx: &RequestContext) -> anyhow::Result<ReindexStatus>;

    /// Get embedding queue status.
    #[rpc(get = "/search/status")]
    async fn queue_status(&self, ctx: &RequestContext) -> anyhow::Result<EmbeddingQueueStatus>;
}
