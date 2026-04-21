//! Chunking and vector search abstractions.
//!
//! The `EmbeddingProvider` trait lives in the `llm` crate alongside other
//! provider traits. This module provides the chunking and vector storage
//! layers that consume embeddings.

mod chunker;
mod vector_store;

pub use chunker::{Chunk, Chunker, RecursiveCharacterChunker};
pub use crate::storage::ids::ChunkId;
pub use vector_store::{
    EntityFilter, EntityTypeMatcher, SearchFilter, SearchQuery, SearchResult, VectorChunk,
    VectorStore,
};
