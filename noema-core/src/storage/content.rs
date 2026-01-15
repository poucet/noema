//! Storage content types - refs-only for database storage
//!
//! `StoredContent` represents what is actually stored in the database.
//! All content is stored as references:
//! - Text → reference to content_blocks table
//! - Images/Audio → reference to blob/asset storage
//! - Documents → reference to documents table
//! - Tool calls/results → inline JSON (no binary content)
//!
//! To convert refs back to full content for LLM/UI, use the `resolve()` method
//! with a `ContentResolver` implementation.

use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use llm::{ContentBlock, ToolCall, ToolResult};
use serde::{Deserialize, Serialize};

use crate::storage::ids::{AssetId, ContentBlockId, DocumentId};

// ============================================================================
// ContentResolver Trait
// ============================================================================

/// Trait for resolving content references to actual content
///
/// Implementations fetch data from the appropriate storage backends:
/// - `get_text()` - Fetches from content_blocks table
/// - `get_asset()` - Fetches from blob storage via asset store
#[async_trait]
pub trait ContentResolver: Send + Sync {
    /// Get text content by content block ID
    async fn get_text(&self, id: &ContentBlockId) -> Result<String>;

    /// Get binary asset data and mime type by asset ID
    async fn get_asset(&self, id: &AssetId) -> Result<(Vec<u8>, String)>;
}

// ============================================================================
// StoredContent - Database representation
// ============================================================================

/// Content stored in the database - all refs, no inline binary data
///
/// This enum represents what is actually stored in the `message_content` table.
/// Each variant maps to a row with the appropriate columns populated.
///
/// To convert to `ContentBlock` for LLM/UI, use `resolve()` with a `ContentResolver`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoredContent {
    /// Text content - stored in content_blocks for deduplication and search
    TextRef {
        content_block_id: ContentBlockId,
    },

    /// Binary asset - images, audio, documents stored in blob storage
    AssetRef {
        asset_id: AssetId,
        mime_type: String,
    },

    /// Reference to a document (for RAG) - displayed as chip in UI,
    /// content injected to LLM separately via DocumentFormatter.
    /// Title is looked up from documents table during resolution.
    DocumentRef {
        document_id: DocumentId,
    },

    /// Tool call - stored as structured JSON
    /// Tool calls may contain binary data which should be externalized
    /// before storage (handled by StorageCoordinator)
    ToolCall(ToolCall),

    /// Tool result - stored as structured JSON
    /// Tool results may contain binary data which should be externalized
    /// before storage (handled by StorageCoordinator)
    ToolResult(ToolResult),
}

impl StoredContent {
    /// Check if this content is a text reference
    pub fn is_text_ref(&self) -> bool {
        matches!(self, StoredContent::TextRef { .. })
    }

    /// Check if this content is an asset reference
    pub fn is_asset_ref(&self) -> bool {
        matches!(self, StoredContent::AssetRef { .. })
    }

    /// Get the content_block_id if this is a text reference
    pub fn content_block_id(&self) -> Option<&ContentBlockId> {
        match self {
            StoredContent::TextRef { content_block_id } => Some(content_block_id),
            _ => None,
        }
    }

    /// Get the asset_id if this is an asset reference
    pub fn asset_id(&self) -> Option<&AssetId> {
        match self {
            StoredContent::AssetRef { asset_id, .. } => Some(asset_id),
            _ => None,
        }
    }

    /// Create a text reference
    pub fn text_ref(content_block_id: ContentBlockId) -> Self {
        StoredContent::TextRef { content_block_id }
    }

    /// Create an asset reference
    pub fn asset_ref(
        asset_id: impl Into<AssetId>,
        mime_type: impl Into<String>,
    ) -> Self {
        StoredContent::AssetRef {
            asset_id: asset_id.into(),
            mime_type: mime_type.into(),
        }
    }

    /// Create a document reference
    pub fn document_ref(document_id: impl Into<DocumentId>) -> Self {
        StoredContent::DocumentRef {
            document_id: document_id.into() ,
        }
    }

    /// Resolve this stored content to a ContentBlock for LLM/UI
    ///
    /// Uses the provided resolver to fetch actual content from storage.
    pub async fn resolve<R: ContentResolver>(&self, resolver: &R) -> Result<ContentBlock> {
        match self {
            StoredContent::TextRef { content_block_id } => {
                let text = resolver.get_text(content_block_id).await?;
                Ok(ContentBlock::Text { text })
            }
            StoredContent::AssetRef {
                asset_id,
                mime_type,
                ..
            } => {
                let (data, _) = resolver.get_asset(asset_id).await?;
                let encoded = STANDARD.encode(&data);

                if mime_type.starts_with("image/") {
                    Ok(ContentBlock::Image {
                        data: encoded,
                        mime_type: mime_type.clone(),
                    })
                } else if mime_type.starts_with("audio/") {
                    Ok(ContentBlock::Audio {
                        data: encoded,
                        mime_type: mime_type.clone(),
                    })
                } else {
                    // For other types (PDFs, etc.), return as image with application mime type
                    // The UI/LLM layer will handle appropriately
                    Ok(ContentBlock::Image {
                        data: encoded,
                        mime_type: mime_type.clone(),
                    })
                }
            }
            StoredContent::DocumentRef { document_id } => {
                Ok(ContentBlock::DocumentRef {
                    id: document_id.to_string(),
                })
            }
            StoredContent::ToolCall(call) => Ok(ContentBlock::ToolCall(call.clone())),
            StoredContent::ToolResult(result) => Ok(ContentBlock::ToolResult(result.clone())),
        }
    }

    /// Resolve without fetching binary data - returns text and refs directly
    ///
    /// Useful for UI display where asset refs should be shown as refs
    /// (the UI will fetch the binary data separately via asset endpoint).
    pub async fn resolve_text_only<R: ContentResolver>(
        &self,
        resolver: &R,
    ) -> Result<ResolvedContent> {
        match self {
            StoredContent::TextRef { content_block_id } => {
                let text = resolver.get_text(content_block_id).await?;
                Ok(ResolvedContent::Text { text })
            }
            StoredContent::AssetRef {
                asset_id,
                mime_type,
            } => Ok(ResolvedContent::AssetRef {
                asset_id: asset_id.clone(),
                mime_type: mime_type.clone(),
            }),
            StoredContent::DocumentRef { document_id } => {
                // Document title is looked up separately by UI
                Ok(ResolvedContent::DocumentRef {
                    document_id: document_id.clone(),
                })
            }
            StoredContent::ToolCall(call) => Ok(ResolvedContent::ToolCall(call.clone())),
            StoredContent::ToolResult(result) => Ok(ResolvedContent::ToolResult(result.clone())),
        }
    }
}

// ============================================================================
// ResolvedContent - For UI with asset refs preserved
// ============================================================================

/// Content resolved for UI display
///
/// Similar to `ContentBlock` but preserves refs so the UI can
/// fetch binary data and document details separately via their endpoints.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResolvedContent {
    Text {
        text: String,
    },
    AssetRef {
        asset_id: AssetId,
        mime_type: String,
    },
    DocumentRef {
        document_id: DocumentId,
    },
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}

// ============================================================================
// InputContent - Raw input from UI before storage
// ============================================================================

/// Content as received from UI, before storage processing
///
/// This is what Tauri/UI sends when the user submits a message.
/// Session converts this to `StoredContent` by:
/// - Storing text in content_blocks → TextRef
/// - Storing base64 image/audio in blob storage → AssetRef
/// - Passing through document refs and existing asset refs
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InputContent {
    /// Plain text to be stored
    Text { text: String },
    /// Reference to an existing document
    DocumentRef { id: DocumentId },
    /// Base64-encoded image data to be stored
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    /// Base64-encoded audio data to be stored
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    /// Reference to already-stored asset (no storage needed)
    AssetRef {
        #[serde(rename = "assetId")]
        asset_id: AssetId,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

// ============================================================================
// StoredPayload - Collection of StoredContent
// ============================================================================

/// A collection of stored content items
///
/// This is the payload format stored in messages. Each item is a `StoredContent`
/// which represents a reference to actual content.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(transparent)]
pub struct StoredPayload {
    pub content: Vec<StoredContent>,
}

impl StoredPayload {
    pub fn new(content: Vec<StoredContent>) -> Self {
        StoredPayload { content }
    }

    /// Check if this payload contains any asset references
    pub fn has_asset_refs(&self) -> bool {
        self.content.iter().any(|c| c.is_asset_ref())
    }

    /// Get all asset IDs referenced in this payload
    pub fn get_asset_refs(&self) -> Vec<&AssetId> {
        self.content.iter().filter_map(|c| c.asset_id()).collect()
    }

    /// Resolve all content items to ContentBlocks for LLM
    pub async fn resolve<R: ContentResolver>(&self, resolver: &R) -> Result<Vec<ContentBlock>> {
        let mut blocks = Vec::with_capacity(self.content.len());
        for item in &self.content {
            blocks.push(item.resolve(resolver).await?);
        }
        Ok(blocks)
    }

    /// Resolve to ResolvedContent (text resolved, assets as refs)
    pub async fn resolve_for_ui<R: ContentResolver>(
        &self,
        resolver: &R,
    ) -> Result<Vec<ResolvedContent>> {
        let mut items = Vec::with_capacity(self.content.len());
        for item in &self.content {
            items.push(item.resolve_text_only(resolver).await?);
        }
        Ok(items)
    }

    /// Convert to LLM ChatPayload after resolving all references
    pub async fn to_chat_payload<R: ContentResolver>(
        &self,
        resolver: &R,
    ) -> Result<llm::ChatPayload> {
        let blocks = self.resolve(resolver).await?;
        Ok(llm::ChatPayload::new(blocks))
    }

    /// Extract tool calls from this payload as JSON string
    pub fn tool_calls_json(&self) -> Option<String> {
        let tool_calls: Vec<&ToolCall> = self
            .content
            .iter()
            .filter_map(|c| match c {
                StoredContent::ToolCall(call) => Some(call),
                _ => None,
            })
            .collect();

        if tool_calls.is_empty() {
            None
        } else {
            serde_json::to_string(&tool_calls).ok()
        }
    }

    /// Extract tool results from this payload as JSON string
    pub fn tool_results_json(&self) -> Option<String> {
        let tool_results: Vec<&ToolResult> = self
            .content
            .iter()
            .filter_map(|c| match c {
                StoredContent::ToolResult(result) => Some(result),
                _ => None,
            })
            .collect();

        if tool_results.is_empty() {
            None
        } else {
            serde_json::to_string(&tool_results).ok()
        }
    }
}

// ============================================================================
// Legacy Support (for migration)
// ============================================================================

/// A message with StoredPayload (preserves refs)
/// Used for sending to UI where refs should be fetched separately
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub role: llm::Role,
    pub payload: StoredPayload,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_text_ref() {
        let text_ref = StoredContent::text_ref(ContentBlockId::from_string("abc123"));
        assert!(text_ref.is_text_ref());
        assert!(!text_ref.is_asset_ref());

        let asset_ref = StoredContent::asset_ref("xyz", "image/png");
        assert!(!asset_ref.is_text_ref());
        assert!(asset_ref.is_asset_ref());
    }

    #[test]
    fn test_content_block_id() {
        let id = ContentBlockId::from_string("test-id");
        let text_ref = StoredContent::text_ref(id.clone());
        assert_eq!(text_ref.content_block_id(), Some(&id));

        let asset_ref = StoredContent::asset_ref("xyz", "image/png");
        assert_eq!(asset_ref.content_block_id(), None);
    }

    #[test]
    fn test_asset_id() {
        let asset_ref = StoredContent::asset_ref("abc123", "image/png");
        assert_eq!(asset_ref.asset_id(), Some(&AssetId::from("abc123")));

        let text_ref = StoredContent::text_ref(ContentBlockId::from_string("xyz"));
        assert_eq!(text_ref.asset_id(), None);
    }

    #[test]
    fn test_stored_payload_has_asset_refs() {
        let payload = StoredPayload::new(vec![
            StoredContent::text_ref(ContentBlockId::from_string("a")),
            StoredContent::asset_ref("b", "image/png"),
        ]);
        assert!(payload.has_asset_refs());

        let text_only = StoredPayload::new(vec![StoredContent::text_ref(
            ContentBlockId::from_string("c"),
        )]);
        assert!(!text_only.has_asset_refs());
    }

    #[test]
    fn test_tool_calls_json() {
        let call = ToolCall {
            id: "call-1".to_string(),
            name: "test_tool".to_string(),
            arguments: serde_json::json!({}),
            extra: serde_json::Value::Null,
        };
        let payload = StoredPayload::new(vec![StoredContent::ToolCall(call)]);

        let json = payload.tool_calls_json();
        assert!(json.is_some());
        assert!(json.unwrap().contains("test_tool"));
    }
}
