//! Types for frontend communication

use llm::{ChatMessage, ContentBlock, Role, ToolResultContent};
use noema_core::storage::ids::{AssetId, ConversationId, DocumentId, SpanId, TurnId, ViewId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    pub capabilities: Vec<String>,
    pub context_window: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ConversationInfo {
    #[ts(type = "string")]
    pub id: ConversationId,
    pub name: Option<String>,
    pub message_count: usize,
    /// Whether this conversation is marked as private (warns before using cloud models)
    pub is_private: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<noema_core::storage::ConversationInfo> for ConversationInfo {
    fn from(info: noema_core::storage::ConversationInfo) -> Self {
        Self {
            id: info.id,
            name: info.name,
            message_count: info.turn_count,
            is_private: info.is_private,
            created_at: info.created_at,
            updated_at: info.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub enum DisplayContent {
    Text(String),
    /// Inline Base64 image
    Image { 
        data: String, 
        #[serde(rename = "mimeType")] 
        mime_type: String 
    },
    /// Inline Base64 audio
    Audio { 
        data: String, 
        #[serde(rename = "mimeType")] 
        mime_type: String
    },
    /// Asset stored in blob storage - URL provided by backend
    AssetRef {
        /// URL to fetch the asset (e.g., noema-asset://localhost/{blob_hash})
        url: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        filename: Option<String>
    },
    /// Reference to a document (shown as chip in UI, content injected to LLM separately)
    DocumentRef {
        #[ts(type = "string")]
        id: DocumentId,
    },
    ToolCall { 
        name: String, 
        id: String, 
        #[ts(type = "unknown")] 
        arguments: serde_json::Value 
    },
    ToolResult { 
        id: String, 
        content: Vec<DisplayToolResultContent> 
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub enum DisplayToolResultContent {
    Text(String),
    Image { 
        data: String, 
        #[serde(rename = "mimeType")]
        mime_type: String 
    },
    Audio { 
        data: String, 
        #[serde(rename = "mimeType")]
        mime_type: String 
    },
}

/// Information about an alternate response for a span set
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct AlternateInfo {
    #[ts(type = "string")]
    pub span_id: SpanId,
    pub model_id: Option<String>,
    pub model_display_name: Option<String>,
    pub message_count: usize,
    pub is_selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct DisplayMessage {
    #[ts(type = "string")]
    pub role: Role,
    pub content: Vec<DisplayContent>,
    /// Turn ID this message belongs to (for switching alternates)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "string | undefined")]
    pub turn_id: Option<TurnId>,
    /// Span ID for this specific message (for fork/edit actions)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "string | undefined")]
    pub span_id: Option<SpanId>,
    /// Available alternates for this message's turn (only populated for assistant messages with alternatives)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternates: Option<Vec<AlternateInfo>>,
}

impl From<&ChatMessage> for DisplayMessage {
    fn from(msg: &ChatMessage) -> Self {
        let content = msg
            .payload
            .content
            .iter()
            .map(DisplayContent::from)
            .collect();

        Self {
            role: msg.role,
            content,
            turn_id: None,
            span_id: None,
            alternates: None,
        }
    }
}

impl DisplayMessage {
    /// Create a DisplayMessage with alternates info from storage
    pub fn with_alternates(
        role: Role,
        content: Vec<DisplayContent>,
        turn_id: TurnId,
        span_id: SpanId,
        alternates: Vec<AlternateInfo>,
    ) -> Self {
        Self {
            role,
            content,
            turn_id: Some(turn_id),
            span_id: Some(span_id),
            alternates: if alternates.len() > 1 { Some(alternates) } else { None },
        }
    }
}

impl From<&ContentBlock> for DisplayContent {
    fn from(block: &ContentBlock) -> Self {
        match block {
            ContentBlock::Text { text } => DisplayContent::Text(text.clone()),
            ContentBlock::Image { data, mime_type } => DisplayContent::Image {
                data: data.clone(),
                mime_type: mime_type.clone(),
            },
            ContentBlock::Audio { data, mime_type } => DisplayContent::Audio {
                data: data.clone(),
                mime_type: mime_type.clone(),
            },
            ContentBlock::ToolCall(call) => DisplayContent::ToolCall {
                name: call.name.clone(),
                id: call.id.clone(),
                arguments: call.arguments.clone(),
            },
            ContentBlock::ToolResult(result) => DisplayContent::ToolResult {
                id: result.tool_call_id.clone(),
                content: result
                    .content
                    .iter()
                    .map(DisplayToolResultContent::from)
                    .collect(),
            },
            ContentBlock::DocumentRef { id } => DisplayContent::DocumentRef {
                id: DocumentId::from(id.clone()),
            },
        }
    }
}

impl From<&ToolResultContent> for DisplayToolResultContent {
    fn from(c: &ToolResultContent) -> Self {
        match c {
            ToolResultContent::Text { text } => DisplayToolResultContent::Text(text.clone()),
            ToolResultContent::Image { data, mime_type } => DisplayToolResultContent::Image {
                data: data.clone(),
                mime_type: mime_type.clone(),
            },
            ToolResultContent::Audio { data, mime_type } => DisplayToolResultContent::Audio {
                data: data.clone(),
                mime_type: mime_type.clone(),
            },
        }
    }
}

// Session ResolvedContent/ResolvedMessage -> Display types
impl From<&noema_core::storage::ResolvedContent> for DisplayContent {
    fn from(content: &noema_core::storage::ResolvedContent) -> Self {
        use noema_core::storage::ResolvedContent;
        match content {
            ResolvedContent::Text { text } => DisplayContent::Text(text.clone()),
            ResolvedContent::Asset {
                blob_hash,
                mime_type,
                filename,
                ..
            } => DisplayContent::AssetRef {
                url: format!(
                    "noema-asset://localhost/{}?mime_type={}",
                    blob_hash,
                    urlencoding::encode(mime_type)
                ),
                mime_type: mime_type.clone(),
                filename: filename.clone(),
            },
            ResolvedContent::Document { document_id, .. } => DisplayContent::DocumentRef {
                id: DocumentId::from(document_id.clone()),
            },
            ResolvedContent::ToolCall(call) => DisplayContent::ToolCall {
                name: call.name.clone(),
                id: call.id.clone(),
                arguments: call.arguments.clone(),
            },
            ResolvedContent::ToolResult(result) => DisplayContent::ToolResult {
                id: result.tool_call_id.clone(),
                content: result
                    .content
                    .iter()
                    .map(DisplayToolResultContent::from)
                    .collect(),
            },
        }
    }
}

impl From<&noema_core::storage::ResolvedMessage> for DisplayMessage {
    fn from(msg: &noema_core::storage::ResolvedMessage) -> Self {
        use noema_core::storage::MessageRole;
        // Map MessageRole to llm::Role (Tool messages displayed as Assistant)
        let role = match msg.role {
            MessageRole::User => Role::User,
            MessageRole::Assistant => Role::Assistant,
            MessageRole::System => Role::System,
            MessageRole::Tool => Role::Assistant, // Tool results rendered like assistant
        };
        Self {
            role,
            content: msg.content.iter().map(DisplayContent::from).collect(),
            turn_id: None,
            span_id: None,
            alternates: None,
        }
    }
}

// MCP server info for frontend
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct McpServerInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_type: String,
    pub is_connected: bool,
    pub needs_oauth_login: bool,
    pub tool_count: usize,
    /// Connection status: "disconnected", "connected", "retrying:N", "stopped:error"
    pub status: String,
    /// Whether to auto-connect on app startup
    pub auto_connect: bool,
    /// Whether to auto-retry with exponential backoff
    pub auto_retry: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub server_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct AddMcpServerRequest {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_type: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub client_secret: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub scopes: Option<Vec<String>>,
    #[serde(default)]
    #[ts(optional)]
    pub use_well_known: Option<bool>,
}

/// Attachment from frontend for message sending
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct Attachment {
    pub data: String,      // base64 encoded

    pub mime_type: String, // e.g., "image/png", "audio/mp3"
}

impl Into<noema_ext::Attachment> for Attachment {
    fn into(self) -> noema_ext::Attachment {
        noema_ext::Attachment {
            data: self.data,
            mime_type: self.mime_type,
        }
    }
}

/// Referenced document for RAG context (legacy - use DisplayInputContent::DocumentRef instead)
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ReferencedDocument {
    pub id: String,
    pub title: String,
}

/// Input content block from frontend - structured content for user messages.
/// This preserves the exact position of document references and attachments inline with text.
///
/// This is the TypeScript-facing type that mirrors `noema_core::storage::InputContent`.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub enum DisplayInputContent {
    /// Plain text segment
    Text {
        text: String
    },
    /// Reference to a document (inline position preserved)
    DocumentRef {
        #[ts(type = "string")]
        id: DocumentId,
    },
    /// Inline base64 image (for new uploads)
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String
    },
    /// Inline base64 audio (for new uploads)
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String
    },
    /// Reference to already-stored asset in blob storage
    AssetRef {
        #[serde(rename = "assetId")]
        #[ts(type = "string")]
        asset_id: AssetId,
        #[serde(rename = "mimeType")]
        mime_type: String
    },
}

impl From<DisplayInputContent> for noema_core::storage::InputContent {
    fn from(block: DisplayInputContent) -> Self {
        use noema_core::storage::InputContent;
        match block {
            DisplayInputContent::Text { text } => InputContent::Text { text },
            DisplayInputContent::DocumentRef { id } => InputContent::DocumentRef { id },
            DisplayInputContent::Image { data, mime_type } => InputContent::Image { data, mime_type },
            DisplayInputContent::Audio { data, mime_type } => InputContent::Audio { data, mime_type },
            DisplayInputContent::AssetRef { asset_id, mime_type } => {
                InputContent::AssetRef { asset_id, mime_type }
            }
        }
    }
}

/// Information about a parallel model response (for UI display)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ParallelAlternateInfo {
    #[ts(type = "string")]
    pub span_id: SpanId,
    pub model_id: String,
    pub model_display_name: String,
    pub message_count: usize,
    pub is_selected: bool,
}

impl From<noema_core::ParallelAlternateInfo> for ParallelAlternateInfo {
    fn from(info: noema_core::ParallelAlternateInfo) -> Self {
        Self {
            span_id: SpanId::from_string(info.span_id),
            model_id: info.model_id,
            model_display_name: info.model_display_name,
            message_count: info.message_count,
            is_selected: info.is_selected,
        }
    }
}

/// Information about a view/branch in a conversation (replaces legacy Thread concept)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ThreadInfoResponse {
    #[ts(type = "string")]
    pub id: ViewId,
    #[ts(type = "string")]
    pub conversation_id: ConversationId,
    #[ts(type = "string | null")]
    pub parent_span_id: Option<TurnId>,
    pub name: Option<String>,
    pub status: String,
    pub created_at: i64,
    /// Whether this is the main view
    pub is_main: bool,
}

impl From<noema_core::storage::ViewInfo> for ThreadInfoResponse {
    fn from(info: noema_core::storage::ViewInfo) -> Self {
        Self {
            id: info.id,
            conversation_id: info.conversation_id,
            parent_span_id: info.forked_at_turn_id,
            name: info.name,
            status: "active".to_string(), // Views are always active
            created_at: info.created_at,
            is_main: info.is_main,
        }
    }
}

/// Streaming message from a specific model during parallel execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ParallelStreamingMessage {
    pub model_id: String,
    pub message: DisplayMessage,
}

/// A model completed its response during parallel execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ParallelModelComplete {
    pub model_id: String,
    pub messages: Vec<DisplayMessage>,
}

/// All parallel models have completed
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ParallelComplete {
    #[ts(type = "string")]
    pub span_set_id: TurnId,
    pub alternates: Vec<ParallelAlternateInfo>,
}

/// Error from a specific model during parallel execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ParallelModelError {
    pub model_id: String,
    pub error: String,
}

// =============================================================================
// Event Payloads - typed payloads for Tauri events
// =============================================================================

/// Payload for streaming_message event
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct StreamingMessageEvent {
    #[ts(type = "string")]
    pub conversation_id: ConversationId,
    pub message: DisplayMessage,
}

/// Payload for message_complete event
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct MessageCompleteEvent {
    #[ts(type = "string")]
    pub conversation_id: ConversationId,
    pub messages: Vec<DisplayMessage>,
}

/// Payload for error event
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ErrorEvent {
    #[ts(type = "string")]
    pub conversation_id: ConversationId,
    pub error: String,
}

/// Payload for model_changed event
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ModelChangedEvent {
    #[ts(type = "string")]
    pub conversation_id: ConversationId,
    pub model: String,
}

/// Payload for history_cleared event
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct HistoryClearedEvent {
    #[ts(type = "string")]
    pub conversation_id: ConversationId,
}

/// Payload for parallel_streaming_message event
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ParallelStreamingMessageEvent {
    #[ts(type = "string")]
    pub conversation_id: ConversationId,
    pub model_id: String,
    pub message: DisplayMessage,
}

/// Payload for parallel_model_complete event
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ParallelModelCompleteEvent {
    #[ts(type = "string")]
    pub conversation_id: ConversationId,
    pub model_id: String,
    pub messages: Vec<DisplayMessage>,
}

/// Payload for parallel_complete event
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ParallelCompleteEvent {
    #[ts(type = "string")]
    pub conversation_id: ConversationId,
    #[ts(type = "string")]
    pub turn_id: TurnId,
    pub alternates: Vec<ParallelAlternateInfo>,
}

/// Payload for parallel_model_error event
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ParallelModelErrorEvent {
    #[ts(type = "string")]
    pub conversation_id: ConversationId,
    pub model_id: String,
    pub error: String,
}

/// Configuration for which tools to enable for a message.
/// Designed to be extensible for future tool set selection.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ToolConfig {
    /// Master toggle: if false, no tools are available regardless of other settings
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Optional list of specific MCP server IDs to include.
    /// If None or empty, all connected servers are used (when enabled=true).
    /// If Some with values, only those servers' tools are available.
    #[serde(default)]
    pub server_ids: Option<Vec<String>>,

    /// Optional list of specific tool names to include (across all servers).
    /// If None, all tools from selected servers are available.
    /// If Some, only these specific tools are available.
    #[serde(default)]
    pub tool_names: Option<Vec<String>>,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod ts_export {
    use super::*;

    #[test]
    fn export_types() {
        ModelInfo::export_all().expect("Failed to export ModelInfo");
        ConversationInfo::export_all().expect("Failed to export ConversationInfo");
        DisplayContent::export_all().expect("Failed to export DisplayContent");
        DisplayToolResultContent::export_all().expect("Failed to export DisplayToolResultContent");
        AlternateInfo::export_all().expect("Failed to export AlternateInfo");
        DisplayMessage::export_all().expect("Failed to export DisplayMessage");
        McpServerInfo::export_all().expect("Failed to export McpServerInfo");
        McpToolInfo::export_all().expect("Failed to export McpToolInfo");
        AddMcpServerRequest::export_all().expect("Failed to export AddMcpServerRequest");
        Attachment::export_all().expect("Failed to export Attachment");
        ParallelAlternateInfo::export_all().expect("Failed to export ParallelAlternateInfo");
        ParallelStreamingMessage::export_all().expect("Failed to export ParallelStreamingMessage");
        ParallelModelComplete::export_all().expect("Failed to export ParallelModelComplete");
        ParallelComplete::export_all().expect("Failed to export ParallelComplete");
        ParallelModelError::export_all().expect("Failed to export ParallelModelError");
        StreamingMessageEvent::export_all().expect("Failed to export StreamingMessageEvent");
        MessageCompleteEvent::export_all().expect("Failed to export MessageCompleteEvent");
        ErrorEvent::export_all().expect("Failed to export ErrorEvent");
        ModelChangedEvent::export_all().expect("Failed to export ModelChangedEvent");
        HistoryClearedEvent::export_all().expect("Failed to export HistoryClearedEvent");
        ParallelStreamingMessageEvent::export_all().expect("Failed to export ParallelStreamingMessageEvent");
        ParallelModelCompleteEvent::export_all().expect("Failed to export ParallelModelCompleteEvent");
        ParallelCompleteEvent::export_all().expect("Failed to export ParallelCompleteEvent");
        ParallelModelErrorEvent::export_all().expect("Failed to export ParallelModelErrorEvent");
        ReferencedDocument::export_all().expect("Failed to export ReferencedDocument");
        DisplayInputContent::export_all().expect("Failed to export DisplayInputContent");
        ThreadInfoResponse::export_all().expect("Failed to export ThreadInfoResponse");
        ToolConfig::export_all().expect("Failed to export ToolConfig");
    }
}
