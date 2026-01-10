//! Content block storage - content-addressed text storage
//!
//! Content blocks are the foundation of the Unified Content Model.
//! All text content (messages, documents, revisions) is stored here
//! with deduplication via SHA-256 hashing.

pub mod types;

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite;

pub use types::{ContentOrigin, ContentType, OriginKind};

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::ContentBlockId;

/// Core content block data (shared between input and stored forms)
#[derive(Clone, Debug, Default)]
pub struct ContentBlock {
    /// The text content
    pub text: String,

    /// Type of content (plain, markdown, typst)
    pub content_type: ContentType,

    /// Whether this content should only be used locally (not sent to cloud models)
    pub is_private: bool,

    /// Origin/provenance information
    pub origin: ContentOrigin,
}

impl ContentBlock {
    /// Create a new plain text content block
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            content_type: ContentType::Plain,
            ..Default::default()
        }
    }

    /// Create a new markdown content block
    pub fn markdown(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            content_type: ContentType::Markdown,
            ..Default::default()
        }
    }

    /// Create a new typst content block
    pub fn typst(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            content_type: ContentType::Typst,
            ..Default::default()
        }
    }

    /// Set the origin information
    pub fn with_origin(mut self, origin: ContentOrigin) -> Self {
        self.origin = origin;
        self
    }

    /// Mark as private (local-only)
    pub fn private(mut self) -> Self {
        self.is_private = true;
        self
    }
}

/// A stored content block with metadata from the database
#[derive(Clone, Debug)]
pub struct StoredContentBlock {
    /// Unique identifier
    pub id: ContentBlockId,

    /// SHA-256 hash of the content
    pub content_hash: String,

    /// The content block data
    pub content: ContentBlock,

    /// When this content was created (unix timestamp ms)
    pub created_at: i64,
}

impl StoredContentBlock {
    /// Get the text content
    pub fn text(&self) -> &str {
        &self.content.text
    }

    /// Get the content type
    pub fn content_type(&self) -> &ContentType {
        &self.content.content_type
    }

    /// Check if private
    pub fn is_private(&self) -> bool {
        self.content.is_private
    }

    /// Get the origin
    pub fn origin(&self) -> &ContentOrigin {
        &self.content.origin
    }
}

/// Result of storing content (may be existing or new)
#[derive(Clone, Debug)]
pub struct StoreResult {
    /// The content block ID
    pub id: ContentBlockId,

    /// The content hash
    pub hash: String,

    /// Whether this was a new insertion (false = deduplicated)
    pub is_new: bool,
}

/// Trait for content block storage operations
#[async_trait]
pub trait ContentBlockStore: Send + Sync {
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
}
