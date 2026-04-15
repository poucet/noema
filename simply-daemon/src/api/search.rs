//! Semantic search API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::embedding_queue::EmbeddingQueueStatus;

/// A search result with document context.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[ts(export, export_to = "admin/src/lib/generated/")]
pub struct SearchHit {
    pub document_id: String,
    pub document_title: String,
    pub document_type: String,
    pub tab_id: String,
    pub chunk_text: String,
    pub chunk_index: u32,
    pub score: f32,
}

/// Status of a reindex operation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[ts(export, export_to = "admin/src/lib/generated/")]
pub struct ReindexStatus {
    pub message: String,
    pub tabs_queued: usize,
}

#[simply_rpc::rpc_service("search")]
#[async_trait]
pub trait SearchApi: Send + Sync {
    /// Semantic search over embedded documents.
    #[rpc(post = "/search")]
    async fn search(&self, query: &str, document_type: Option<String>, top_k: Option<usize>) -> anyhow::Result<Vec<SearchHit>>;

    /// Re-embed all documents. Runs in the background; returns immediately.
    #[rpc(post = "/search/reindex", no_tool)]
    async fn reindex(&self) -> anyhow::Result<ReindexStatus>;

    /// Get embedding queue status.
    #[rpc(get = "/search/status")]
    async fn queue_status(&self) -> anyhow::Result<EmbeddingQueueStatus>;
}
