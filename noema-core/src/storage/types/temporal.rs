//! Temporal query types
//!
//! Types for time-based entity queries supporting:
//! - Slash command/search (lazy loading - metadata only)
//! - Manual injection (eager loading - with content)
//! - Future MCP/RAG (eager loading - with content)

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::storage::ids::EntityId;
use crate::storage::types::EntityType;

// ============================================================================
// TemporalQuery
// ============================================================================

/// Query parameters for time-based entity search
///
/// The caller controls the time range - no hardcoded "recent" concept.
/// Use `include_content` to control lazy vs eager loading.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporalQuery {
    /// Start of time range (unix timestamp ms)
    pub start: i64,
    /// End of time range (unix timestamp ms)
    pub end: i64,
    /// Filter by entity types (None = all types)
    pub entity_types: Option<Vec<EntityType>>,
    /// Whether to load content previews (eager) or just metadata (lazy)
    pub include_content: bool,
    /// Maximum number of results (None = no limit)
    pub limit: Option<u32>,
}

impl TemporalQuery {
    /// Create a new temporal query for a time range
    pub fn new(start: i64, end: i64) -> Self {
        Self {
            start,
            end,
            entity_types: None,
            include_content: false,
            limit: None,
        }
    }

    /// Filter to specific entity types
    pub fn with_types(mut self, types: Vec<EntityType>) -> Self {
        self.entity_types = Some(types);
        self
    }

    /// Include content previews (eager loading)
    pub fn with_content(mut self) -> Self {
        self.include_content = true;
        self
    }

    /// Limit number of results
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
}

// ============================================================================
// TemporalEntity
// ============================================================================

/// An entity returned from a temporal query
///
/// Contains entity metadata and optionally a content preview.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporalEntity {
    /// Entity ID
    pub entity_id: EntityId,
    /// Type of entity (conversation, document, asset, etc.)
    pub entity_type: EntityType,
    /// Display name (if set)
    pub name: Option<String>,
    /// When entity was created (unix timestamp ms)
    pub created_at: i64,
    /// When entity was last updated (unix timestamp ms)
    pub updated_at: i64,
    /// Content preview (only populated if query.include_content = true)
    pub content_preview: Option<ContentPreview>,
}

// ============================================================================
// ContentPreview
// ============================================================================

/// A preview of an entity's content
///
/// For conversations: latest message text
/// For documents: latest revision text
/// For assets: metadata only (size, mime_type), never the blob
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentPreview {
    /// Type of content
    pub kind: ContentKind,
    /// Text content (for messages and revisions)
    pub text: Option<String>,
    /// Size in bytes (for assets)
    pub byte_size: Option<u64>,
    /// MIME type (for assets)
    pub mime_type: Option<String>,
}

impl ContentPreview {
    /// Create a message content preview
    pub fn message(text: impl Into<String>) -> Self {
        Self {
            kind: ContentKind::Message,
            text: Some(text.into()),
            byte_size: None,
            mime_type: None,
        }
    }

    /// Create a document revision content preview
    pub fn revision(text: impl Into<String>) -> Self {
        Self {
            kind: ContentKind::Revision,
            text: Some(text.into()),
            byte_size: None,
            mime_type: None,
        }
    }

    /// Create an asset metadata preview (no blob data)
    pub fn asset(byte_size: u64, mime_type: impl Into<String>) -> Self {
        Self {
            kind: ContentKind::Asset,
            text: None,
            byte_size: Some(byte_size),
            mime_type: Some(mime_type.into()),
        }
    }
}

// ============================================================================
// ContentKind
// ============================================================================

/// Type of content in a preview
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentKind {
    /// Conversation message content
    Message,
    /// Document revision content
    Revision,
    /// Asset metadata (size, mime_type only)
    Asset,
}

// ============================================================================
// ActivitySummary
// ============================================================================

/// Summary of activity in a time range
///
/// Provides counts and statistics without loading full content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivitySummary {
    /// Start of time range (unix timestamp ms)
    pub start: i64,
    /// End of time range (unix timestamp ms)
    pub end: i64,
    /// Count of entities by type
    pub entity_counts: HashMap<EntityType, u32>,
    /// Total messages created in range
    pub total_messages: u32,
    /// Total document revisions in range
    pub total_revisions: u32,
}

impl ActivitySummary {
    /// Create a new empty activity summary
    pub fn new(start: i64, end: i64) -> Self {
        Self {
            start,
            end,
            entity_counts: HashMap::new(),
            total_messages: 0,
            total_revisions: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temporal_query_builder() {
        let query = TemporalQuery::new(1000, 2000)
            .with_types(vec![EntityType::conversation(), EntityType::document()])
            .with_content()
            .with_limit(50);

        assert_eq!(query.start, 1000);
        assert_eq!(query.end, 2000);
        assert_eq!(query.entity_types.as_ref().unwrap().len(), 2);
        assert!(query.include_content);
        assert_eq!(query.limit, Some(50));
    }

    #[test]
    fn test_content_preview_message() {
        let preview = ContentPreview::message("Hello world");
        assert_eq!(preview.kind, ContentKind::Message);
        assert_eq!(preview.text.as_deref(), Some("Hello world"));
        assert!(preview.byte_size.is_none());
    }

    #[test]
    fn test_content_preview_asset() {
        let preview = ContentPreview::asset(1024, "image/png");
        assert_eq!(preview.kind, ContentKind::Asset);
        assert!(preview.text.is_none());
        assert_eq!(preview.byte_size, Some(1024));
        assert_eq!(preview.mime_type.as_deref(), Some("image/png"));
    }
}
