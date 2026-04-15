//! Vector storage trait for embedding search.

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{ChunkId, DocumentId, TabId, UserId};

/// A chunk of document text with its embedding vector.
#[derive(Debug, Clone)]
pub struct VectorChunk {
    pub id: ChunkId,
    pub document_id: DocumentId,
    pub tab_id: TabId,
    pub document_type: String,
    pub user_id: UserId,
    pub chunk_index: u32,
    pub text: String,
    pub embedding: Vec<f32>,
}

/// Query for vector similarity search.
pub struct SearchQuery {
    pub vector: Vec<f32>,
    pub top_k: usize,
    pub filter: Option<SearchFilter>,
}

/// Filters applied before vector search (fast, indexed columns).
pub struct SearchFilter {
    pub document_type: Option<String>,
    pub user_id: Option<UserId>,
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

    /// Delete all chunks for a given document tab.
    async fn delete_by_tab(&self, tab_id: &TabId) -> Result<()>;

    /// Delete all chunks for a given document.
    async fn delete_by_document(&self, document_id: &DocumentId) -> Result<()>;

    /// Delete all chunks (used during reindex).
    async fn delete_all(&self) -> Result<()>;
}
