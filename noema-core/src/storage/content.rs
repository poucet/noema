//! Storage content types with blob reference support
//!
//! These types are used for serialization to the database. They extend the LLM
//! content types with blob references for Content-Addressable Storage (CAS).

use llm::{ContentBlock, Role, ToolCall, ToolResult};
use serde::{Deserialize, Serialize};

/// Content block with blob reference support for storage
///
/// This type mirrors `llm::ContentBlock` but adds an `AssetRef` variant for
/// content stored in blob storage. When loading from the database,
/// asset refs must be resolved to inline data before sending to LLM providers.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoredContent {
    Text { text: String },
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
    /// Asset stored in blob storage (CAS) - images, audio, documents, etc.
    AssetRef {
        asset_id: String,
        mime_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
    },
    /// Reference to a document (for RAG) - displayed as chip, content injected to LLM separately
    DocumentRef {
        id: String,
        title: String,
    },
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}

impl StoredContent {
    /// Check if this content references a blob
    pub fn is_asset_ref(&self) -> bool {
        matches!(self, StoredContent::AssetRef { .. })
    }

    /// Get the asset_id if this is an asset reference
    pub fn asset_id(&self) -> Option<&str> {
        match self {
            StoredContent::AssetRef { asset_id, .. } => Some(asset_id),
            _ => None,
        }
    }

    /// Create an asset reference
    pub fn asset_ref(
        asset_id: impl Into<String>,
        mime_type: impl Into<String>,
        filename: Option<String>,
    ) -> Self {
        StoredContent::AssetRef {
            asset_id: asset_id.into(),
            mime_type: mime_type.into(),
            filename,
        }
    }
}

/// Convert from LLM ContentBlock to StoredContent
impl From<ContentBlock> for StoredContent {
    fn from(block: ContentBlock) -> Self {
        match block {
            ContentBlock::Text { text } => StoredContent::Text { text },
            ContentBlock::Image { data, mime_type } => StoredContent::Image { data, mime_type },
            ContentBlock::Audio { data, mime_type } => StoredContent::Audio { data, mime_type },
            ContentBlock::DocumentRef { id, title } => StoredContent::DocumentRef { id, title },
            ContentBlock::ToolCall(call) => StoredContent::ToolCall(call),
            ContentBlock::ToolResult(result) => StoredContent::ToolResult(result),
        }
    }
}

/// Convert from StoredContent to LLM ContentBlock
///
/// This conversion fails if the content is an asset reference (needs blob resolution).
/// DocumentRef converts directly since ContentBlock now supports it.
impl TryFrom<StoredContent> for ContentBlock {
    type Error = UnresolvedAssetError;

    fn try_from(stored: StoredContent) -> Result<Self, Self::Error> {
        match stored {
            StoredContent::Text { text } => Ok(ContentBlock::Text { text }),
            StoredContent::Image { data, mime_type } => Ok(ContentBlock::Image { data, mime_type }),
            StoredContent::Audio { data, mime_type } => Ok(ContentBlock::Audio { data, mime_type }),
            StoredContent::DocumentRef { id, title } => Ok(ContentBlock::DocumentRef { id, title }),
            StoredContent::ToolCall(call) => Ok(ContentBlock::ToolCall(call)),
            StoredContent::ToolResult(result) => Ok(ContentBlock::ToolResult(result)),
            StoredContent::AssetRef { asset_id, .. } => Err(UnresolvedAssetError(asset_id)),
        }
    }
}

/// Error when trying to convert an unresolved asset reference
#[derive(Debug, Clone)]
pub struct UnresolvedAssetError(pub String);

impl std::fmt::Display for UnresolvedAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unresolved asset reference: {}", self.0)
    }
}

impl std::error::Error for UnresolvedAssetError {}

pub type UnresolvedBlobError = UnresolvedAssetError;

/// Stored payload with blob reference support
///
/// Episteme stores content as a plain array `[{...}]`, so we use `#[serde(transparent)]`
/// to serialize/deserialize directly as an array without the `content` wrapper.
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
    pub fn get_asset_refs(&self) -> Vec<&str> {
        self.content.iter().filter_map(|c| c.asset_id()).collect()
    }

    /// Resolve all asset references using an async resolver function.
    ///
    /// The resolver takes an asset_id and returns the raw binary data.
    /// References are converted to inline Base64 content based on mime type.
    pub async fn resolve<F, Fut, E>(&mut self, resolver: F) -> Result<(), E>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<u8>, E>>,
    {
        use base64::{engine::general_purpose::STANDARD, Engine};

        for content in &mut self.content {
            *content = match content {
                StoredContent::AssetRef { asset_id, mime_type, .. } => {
                    let data = resolver(asset_id.clone()).await?;
                    let encoded = STANDARD.encode(&data);

                    // Convert to appropriate inline type based on mime type
                    if mime_type.starts_with("image/") {
                        StoredContent::Image {
                            data: encoded,
                            mime_type: mime_type.clone(),
                        }
                    } else if mime_type.starts_with("audio/") {
                        StoredContent::Audio {
                            data: encoded,
                            mime_type: mime_type.clone(),
                        }
                    } else {
                        // For other types (documents, etc.), keep as-is for now
                        // TODO: Handle PDFs, text files, etc.
                        continue;
                    }
                }
                _ => continue,
            };
        }
        Ok(())
    }

    /// Convert to LLM ChatPayload after resolving all blob references
    pub fn to_chat_payload(&self) -> Result<llm::ChatPayload, UnresolvedBlobError> {
        let blocks: Result<Vec<ContentBlock>, _> = self
            .content
            .iter()
            .cloned()
            .map(ContentBlock::try_from)
            .collect();
        Ok(llm::ChatPayload::new(blocks?))
    }

    /// Extract tool calls from this payload as JSON string (for new messages table)
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

    /// Extract tool results from this payload as JSON string (for new messages table)
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

/// Convert from LLM ChatPayload to StoredPayload
impl From<llm::ChatPayload> for StoredPayload {
    fn from(payload: llm::ChatPayload) -> Self {
        StoredPayload {
            content: payload.content.into_iter().map(StoredContent::from).collect(),
        }
    }
}

/// A message with StoredPayload (preserves asset refs)
/// Used for sending to UI where refs should be fetched separately
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub role: Role,
    pub payload: StoredPayload,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_content_roundtrip() {
        let original = ContentBlock::Text {
            text: "Hello".to_string(),
        };
        let stored: StoredContent = original.clone().into();
        let back: ContentBlock = stored.try_into().unwrap();
        assert!(matches!(back, ContentBlock::Text { text } if text == "Hello"));
    }

    #[test]
    fn test_asset_ref_conversion_fails() {
        let stored = StoredContent::asset_ref("abc123", "image/png", None);
        let result: Result<ContentBlock, _> = stored.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_is_asset_ref() {
        assert!(StoredContent::asset_ref("abc", "image/png", None).is_asset_ref());
        assert!(StoredContent::asset_ref("abc", "audio/wav", None).is_asset_ref());
        assert!(StoredContent::asset_ref("abc", "application/pdf", Some("doc.pdf".into())).is_asset_ref());
        assert!(!StoredContent::Text {
            text: "hi".to_string()
        }
        .is_asset_ref());
    }
}
