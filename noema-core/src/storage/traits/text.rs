//! TextStore trait for content-addressed text storage

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::ContentBlockId;
use crate::storage::types::content_block::{ContentBlock, StoredContentBlock, StoreResult};

/// Trait for content block storage operations
#[async_trait]
pub trait TextStore: Send + Sync {
    /// Store text content, returning ID and hash
    ///
    /// If content with the same hash already exists, returns the existing ID
    /// (content deduplication).
    async fn store(&self, content: ContentBlock) -> Result<StoreResult>;

    /// Get a content block by ID
    async fn get(&self, id: &ContentBlockId) -> Result<Option<StoredContentBlock>>;

    /// Get just the text content by ID (lightweight)
    async fn get_text(&self, id: &ContentBlockId) -> Result<Option<String>>;

    /// Check if a content block exists
    async fn exists(&self, id: &ContentBlockId) -> Result<bool>;

    /// Find content block by hash (for deduplication checks)
    async fn find_by_hash(&self, hash: &str) -> Result<Option<ContentBlockId>>;

    /// Get text content by ID, returning error if not found
    ///
    /// This is a convenience method for cases where the content block
    /// must exist (e.g., resolving references during session loading).
    async fn require_text(&self, id: &ContentBlockId) -> Result<String> {
        self.get_text(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Content block not found: {}", id.as_str()))
    }
}
