//! Types for frontend communication

use llm::{ChatMessage, ContentBlock, Role, ToolResultContent};
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
    pub id: String,
    pub name: Option<String>,
    pub message_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<noema_core::ConversationInfo> for ConversationInfo {
    fn from(info: noema_core::ConversationInfo) -> Self {
        Self {
            id: info.id,
            name: info.name,
            message_count: info.message_count,
            created_at: info.created_at,
            updated_at: info.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/", rename_all = "camelCase")]
pub enum DisplayContent {
    Text(String),
    /// Inline Base64 image
    Image { data: String, #[serde(rename = "mimeType")] mime_type: String },
    /// Inline Base64 audio
    Audio { data: String, #[serde(rename = "mimeType")] mime_type: String },
    /// Asset stored in blob storage - client should fetch via asset API
    AssetRef {
        #[serde(rename = "assetId")]
        asset_id: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        filename: Option<String>
    },
    /// Reference to a document (shown as chip in UI, content injected to LLM separately)
    DocumentRef {
        id: String,
        title: String,
    },
    ToolCall { name: String, id: String, #[ts(type = "unknown")] arguments: serde_json::Value },
    ToolResult { id: String, content: Vec<DisplayToolResultContent> },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub enum DisplayToolResultContent {
    Text(String),
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
}

/// Information about an alternate response for a span set
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct AlternateInfo {
    pub span_id: String,
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
    /// Span set ID this message belongs to (for switching alternates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_set_id: Option<String>,
    /// Span ID for this specific message (for fork/edit actions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    /// Available alternates for this message's span set (only populated for assistant messages with alternatives)
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
            span_set_id: None,
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
        span_set_id: String,
        span_id: String,
        alternates: Vec<AlternateInfo>,
    ) -> Self {
        Self {
            role: role,
            content,
            span_set_id: Some(span_set_id),
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
            ContentBlock::DocumentRef { id, title } => DisplayContent::DocumentRef {
                id: id.clone(),
                title: title.clone(),
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

impl From<noema_core::storage::StoredContent> for DisplayContent {
    fn from(content: noema_core::storage::StoredContent) -> Self {
        use noema_core::storage::StoredContent;
        match content {
            StoredContent::Text { text } => DisplayContent::Text(text.clone()),
            StoredContent::Image { data, mime_type } => DisplayContent::Image {
                data: data,
                mime_type: mime_type,
            },
            StoredContent::Audio { data, mime_type } => DisplayContent::Audio {
                data: data.clone(),
                mime_type: mime_type.clone(),
            },
            StoredContent::AssetRef { asset_id, mime_type, filename } => DisplayContent::AssetRef {
                asset_id: asset_id.clone(),
                mime_type: mime_type.clone(),
                filename: filename.clone(),
            },
            StoredContent::DocumentRef { id, title } => DisplayContent::DocumentRef {
                id: id.clone(),
                title: title.clone(),
            },
            StoredContent::ToolCall(call) => DisplayContent::ToolCall {
                name: call.name.clone(),
                id: call.id.clone(),
                arguments: call.arguments.clone(),
            },
            StoredContent::ToolResult(result) => DisplayContent::ToolResult {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub use_well_known: bool,
}

/// Attachment from frontend for message sending
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct Attachment {
    #[serde(default)]
    pub name: String,      // filename
    pub data: String,      // base64 encoded
    pub mime_type: String, // e.g., "image/png", "audio/mp3"
    #[serde(default)]
    pub size: usize,       // size in bytes
}

/// Information about a parallel model response (for UI display)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ParallelAlternateInfo {
    pub span_id: String,
    pub model_id: String,
    pub model_display_name: String,
    pub message_count: usize,
    pub is_selected: bool,
}

impl From<noema_core::ParallelAlternateInfo> for ParallelAlternateInfo {
    fn from(info: noema_core::ParallelAlternateInfo) -> Self {
        Self {
            span_id: info.span_id,
            model_id: info.model_id,
            model_display_name: info.model_display_name,
            message_count: info.message_count,
            is_selected: info.is_selected,
        }
    }
}

/// Information about a thread/branch in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct ThreadInfoResponse {
    pub id: String,
    pub conversation_id: String,
    pub parent_span_id: Option<String>,
    pub name: Option<String>,
    pub status: String,
    pub created_at: i64,
    /// Whether this is the main thread (no parent_span_id)
    pub is_main: bool,
}

impl From<noema_core::ThreadInfo> for ThreadInfoResponse {
    fn from(info: noema_core::ThreadInfo) -> Self {
        Self {
            id: info.id,
            conversation_id: info.conversation_id,
            parent_span_id: info.parent_span_id.clone(),
            name: info.name,
            status: info.status,
            created_at: info.created_at,
            is_main: info.parent_span_id.is_none(),
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
    pub span_set_id: String,
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
        ThreadInfoResponse::export_all().expect("Failed to export ThreadInfoResponse");
    }
}
