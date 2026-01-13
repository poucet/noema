//! Session types for the new DB-agnostic session abstraction
//!
//! Key types:
//! - `ResolvedContent` - Content with text resolved, assets/docs cached lazily
//! - `ResolvedMessage` - A message with resolved content

use llm::{ContentBlock, Role, ToolCall, ToolResult};

use crate::storage::ids::{AssetId, DocumentId, TurnId};
use crate::storage::types::{BlobHash};

// ============================================================================
// ResolvedMessage - cached for display and LLM
// ============================================================================

/// A resolved message with cached content
#[derive(Clone, Debug)]
pub struct ResolvedMessage {
    pub role: Role,
    pub content: Vec<ResolvedContent>,
    /// Turn this message belongs to (for truncation)
    pub turn_id: TurnId,
}

impl ResolvedMessage {
    pub fn new(role: Role, content: Vec<ResolvedContent>, turn_id: TurnId) -> Self {
        Self { role, content, turn_id }
    }
}

// ============================================================================
// ResolvedContent - text resolved, assets/docs cached lazily
// ============================================================================

/// Content with text resolved, assets/docs cached lazily for LLM
///
/// This enum serves both display and LLM needs:
/// - Display: Uses the variant fields directly (ignores `resolved`)
/// - LLM: Uses cached `resolved` ContentBlock, populates on first access
#[derive(Clone, Debug)]
pub enum ResolvedContent {
    /// Text content - already resolved, no caching needed
    Text { text: String },

    /// Asset reference with lazy LLM resolution
    Asset {
        asset_id: AssetId,
        /// Blob hash for serving via asset protocol
        blob_hash: BlobHash,
        mime_type: String,
        /// Cached base64-encoded ContentBlock for LLM - populated on first use
        resolved: Option<ContentBlock>,
    },

    /// Document reference with lazy LLM resolution
    Document {
        document_id: DocumentId,
        /// Cached formatted ContentBlock for LLM - populated on first use
        resolved: Option<ContentBlock>,
    },

    /// Tool call - no resolution needed
    ToolCall(ToolCall),

    /// Tool result - no resolution needed
    ToolResult(ToolResult),
}

impl ResolvedContent {
    /// Create a text content item
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Create an asset reference (potentially resolved)
    pub fn asset(
        asset_id: impl Into<AssetId>,
        blob_hash: impl Into<BlobHash>,
        mime_type: impl Into<String>,
        resolved: Option<ContentBlock>,
    ) -> Self {
        Self::Asset {
            asset_id: asset_id.into(),
            blob_hash: blob_hash.into(),
            mime_type: mime_type.into(),
            resolved,
        }
    }

    /// Create a document reference (unresolved)
    pub fn document(document_id: impl Into<DocumentId>) -> Self {
        Self::Document {
            document_id: document_id.into(),
            resolved: None,
        }
    }

    /// Create a tool call
    pub fn tool_call(call: ToolCall) -> Self {
        Self::ToolCall(call)
    }

    /// Create a tool result
    pub fn tool_result(result: ToolResult) -> Self {
        Self::ToolResult(result)
    }

    /// Check if this content needs LLM resolution
    pub fn needs_resolution(&self) -> bool {
        match self {
            Self::Asset { resolved, .. } => resolved.is_none(),
            Self::Document { resolved, .. } => resolved.is_none(),
            _ => false,
        }
    }

    /// Get the cached ContentBlock if available (for assets/documents)
    pub fn cached_block(&self) -> Option<&ContentBlock> {
        match self {
            Self::Asset { resolved, .. } => resolved.as_ref(),
            Self::Document { resolved, .. } => resolved.as_ref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolved_content_text() {
        let content = ResolvedContent::text("hello");
        assert!(!content.needs_resolution());
        assert!(content.cached_block().is_none());
    }

    #[test]
    fn test_resolved_content_asset_unresolved() {
        let blob_hash: BlobHash = "abc123hash".parse().unwrap();
        let content = ResolvedContent::asset("asset-123", blob_hash, "image/png", None);
        assert!(content.needs_resolution());
        assert!(content.cached_block().is_none());
    }

    #[test]
    fn test_resolved_content_document_unresolved() {
        let content = ResolvedContent::document("doc-456");
        assert!(content.needs_resolution());
        assert!(content.cached_block().is_none());
    }
}
