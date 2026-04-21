//! TextStore trait for text content storage

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::{Hashed, Stored};
use crate::storage::ids::ContentBlockId;
use crate::storage::types::ContentBlock;

/// Stored representation of a content block (immutable)
pub type StoredTextBlock = Stored<ContentBlockId, Hashed<ContentBlock>>;

/// Trait for content block storage operations
#[async_trait]
pub trait TextStore: Send + Sync {
    /// Store text content, returning the new block's ID
    ///
    /// Each store creates a new entry - no deduplication by hash,
    /// since metadata (origin, content_type, is_private) may differ.
    async fn store(&self, content: ContentBlock) -> Result<ContentBlockId>;

    /// Get a content block by ID
    async fn get(&self, id: &ContentBlockId) -> Result<Option<StoredTextBlock>>;

    /// Get just the text content by ID (lightweight)
    async fn get_text(&self, id: &ContentBlockId) -> Result<Option<String>>;

    /// Batch-fetch text by a set of content-block IDs. Returns a map of
    /// id → text for ids that have non-null text; missing ids are dropped.
    /// Implementations should use a single underlying query (e.g. sqlite
    /// `WHERE id IN (…)`) to avoid N round-trips.
    async fn get_texts(
        &self,
        ids: &[ContentBlockId],
    ) -> Result<std::collections::HashMap<ContentBlockId, String>>;

    /// Check if a content block exists
    async fn exists(&self, id: &ContentBlockId) -> Result<bool>;
}
