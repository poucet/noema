//! Vector storage trait for embedding search.
//!
//! Chunks key on `content_block_id` — the immutable content record that
//! backs a text-bearing entity. Owning-entity metadata (`entity_id`,
//! `entity_kind`, `title`) is denormalised onto each chunk so that a
//! single SQL query can filter by entity type without a join.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::storage::ids::{ChunkId, ContentBlockId, EntityId};

/// A chunk of entity text with its embedding vector.
///
/// Per-chunk metadata is kept as minimal as possible. We resolve the
/// owning entity (for title, access, body) at query time rather than
/// cache snapshots here — that way visibility / title / body edits
/// don't require reindexing. The only denormalised field is
/// `entity_kind`, because filtering by kind is a SQL predicate that has
/// to execute before we know which entities to fetch.
#[derive(Debug, Clone)]
pub struct VectorChunk {
    pub id: ChunkId,
    /// Content block the chunk was cut from. Used to evict stale chunks
    /// when content is replaced (content blocks are immutable — a new
    /// block id means new content).
    pub content_block_id: ContentBlockId,
    /// Entity that owns the content block.
    pub entity_id: EntityId,
    /// Namespaced entity kind (e.g. `"document::note"`, `"document::tab"`).
    /// Denormalised so `EntityFilter` predicates stay a single SQL
    /// clause — otherwise every candidate would need an entity fetch.
    pub entity_kind: String,
    pub embedding: Vec<f32>,
}

/// Query for vector similarity search.
pub struct SearchQuery {
    pub vector: Vec<f32>,
    pub top_k: usize,
    pub filter: Option<SearchFilter>,
}

/// Filters applied at vector-search time. Access control is **not**
/// here — the caller intersects the returned `entity_id`s with what the
/// user can actually see (checked against live entity state).
pub struct SearchFilter {
    pub entity_filter: Option<EntityFilter>,
}

/// Include/exclude predicates over `entity_kind`. Applied at query time.
///
/// - `include` is empty → everything is included (subject to `exclude`).
/// - A chunk matches iff `include.any(kind) && !exclude.any(kind)`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityFilter {
    pub include: Vec<EntityTypeMatcher>,
    pub exclude: Vec<EntityTypeMatcher>,
}

/// How to match an entity kind. `Prefix("document::")` matches every
/// subtype in the `document::` namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum EntityTypeMatcher {
    Exact(String),
    Prefix(String),
}

impl EntityTypeMatcher {
    pub fn matches(&self, kind: &str) -> bool {
        match self {
            EntityTypeMatcher::Exact(k) => kind == k,
            EntityTypeMatcher::Prefix(p) => kind.starts_with(p),
        }
    }
}

impl EntityFilter {
    pub fn matches(&self, kind: &str) -> bool {
        let included = self.include.is_empty() || self.include.iter().any(|m| m.matches(kind));
        let excluded = self.exclude.iter().any(|m| m.matches(kind));
        included && !excluded
    }
}

/// A search result with similarity score.
pub struct SearchResult {
    pub chunk: VectorChunk,
    /// Similarity score (0.0 - 1.0, higher is more similar).
    pub score: f32,
}

/// Trait for vector storage and similarity search.
///
/// The default implementation uses sqlite-vec. Alternative implementations
/// (e.g. ChromaDB) can be swapped in by implementing this trait.
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Store a batch of chunks with their embeddings.
    async fn upsert(&self, chunks: &[VectorChunk]) -> Result<()>;

    /// Search for similar chunks, with optional filters.
    async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>>;

    /// Delete all chunks for a given content block. Called when content
    /// is replaced (entities get a fresh `content_block_id` on write).
    async fn delete_by_content_block(&self, content_block_id: &ContentBlockId) -> Result<()>;

    /// Delete all chunks for a given entity. Called when an entity is
    /// deleted — removes chunks regardless of which content block they
    /// came from.
    async fn delete_by_entity(&self, entity_id: &EntityId) -> Result<()>;

    /// Delete all chunks (used during reindex).
    async fn delete_all(&self) -> Result<()>;
}
