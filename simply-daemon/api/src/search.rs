//! Semantic search API — content-block-keyed, entity-type filtered.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use simply_rpc::RequestContext;
#[cfg(feature = "ts")]
use ts_rs::TS;

use crate::types::EmbeddingQueueStatus;

/// How to match an entity kind in an `EntityFilter`.
/// `Prefix("document::")` matches every subtype in the namespace.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum EntityTypeMatcher {
    Exact(String),
    Prefix(String),
}

/// Include/exclude predicates over entity kind. Applied at query time.
///
/// - `include` is empty → everything is included (subject to `exclude`).
/// - A chunk matches iff `include.any(kind) && !exclude.any(kind)`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
#[serde(rename_all = "camelCase")]
pub struct EntityFilter {
    #[serde(default)]
    pub include: Vec<EntityTypeMatcher>,
    #[serde(default)]
    pub exclude: Vec<EntityTypeMatcher>,
}

/// A search result pointing at a relevant content block. Hits are
/// deduped by `content_block_id` server-side — each block appears at
/// most once with the best chunk score. Titles / bodies are fetched
/// fresh via `EntityApi::get_entity` / `get_entity_content(entity_id)`
/// so renames don't need a reindex.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub content_block_id: String,
    pub entity_id: String,
    /// Namespaced entity kind (e.g. `"document::note"`). Only piece of
    /// entity metadata cached on the index.
    pub entity_kind: String,
    pub score: f32,
}

/// Search request.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub top_k: Option<usize>,
    /// Optional include/exclude filter over entity kinds. Absent =
    /// include everything.
    #[serde(default)]
    pub entity_filter: Option<EntityFilter>,
}

/// Status of a reindex operation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
#[serde(rename_all = "camelCase")]
pub struct ReindexStatus {
    pub message: String,
    pub entities_queued: usize,
}

#[simply_rpc::rpc_service("search")]
#[async_trait]
pub trait SearchApi: Send + Sync {
    /// Semantic search over embedded entity content.
    #[rpc(post = "/search")]
    async fn search(&self, ctx: &RequestContext, request: SearchRequest) -> anyhow::Result<Vec<SearchHit>>;

    /// Re-embed all content-bearing entities. Runs in the background;
    /// returns immediately with a count of jobs queued.
    #[rpc(post = "/search/reindex", no_tool)]
    async fn reindex(&self, ctx: &RequestContext) -> anyhow::Result<ReindexStatus>;

    /// Get embedding queue status.
    #[rpc(get = "/search/status", no_tool)]
    async fn queue_status(&self, ctx: &RequestContext) -> anyhow::Result<EmbeddingQueueStatus>;
}
