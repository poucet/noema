//! Types for frontend communication

use llm::{ChatMessage, ChatPayload, ContentBlock, Role, ToolResultContent};
use noema_core::storage::StoredContent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayContent {
    Text(String),
    /// Inline Base64 image
    Image { data: String, mime_type: String },
    /// Inline Base64 audio
    Audio { data: String, mime_type: String },
    /// Asset stored in blob storage - client should fetch via asset API
    AssetRef { asset_id: String, mime_type: String, filename: Option<String> },
    ToolCall { name: String, id: String, arguments: serde_json::Value },
    ToolResult { id: String, content: Vec<DisplayToolResultContent> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayToolResultContent {
    Text(String),
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayMessage {
    pub role: String,
    pub content: Vec<DisplayContent>,
}

impl DisplayMessage {
    pub fn from_chat_message(msg: &ChatMessage) -> Self {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        };

        let content = msg
            .payload
            .content
            .iter()
            .map(content_block_to_display)
            .collect();

        Self {
            role: role.to_string(),
            content,
        }
    }

    pub fn from_payload(payload: &ChatPayload) -> Self {
        let content = payload
            .content
            .iter()
            .map(content_block_to_display)
            .collect();

        Self {
            role: "user".to_string(),
            content,
        }
    }
}

fn content_block_to_display(block: &ContentBlock) -> DisplayContent {
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
                .map(tool_result_content_to_display)
                .collect(),
        },
    }
}

fn tool_result_content_to_display(c: &ToolResultContent) -> DisplayToolResultContent {
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

/// Convert StoredContent (with blob refs) to DisplayContent
/// Used when sending messages to the frontend - refs are preserved so client can fetch them
pub fn stored_content_to_display(content: &StoredContent) -> DisplayContent {
    match content {
        StoredContent::Text { text } => DisplayContent::Text(text.clone()),
        StoredContent::Image { data, mime_type } => DisplayContent::Image {
            data: data.clone(),
            mime_type: mime_type.clone(),
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
                .map(tool_result_content_to_display)
                .collect(),
        },
    }
}

// MCP server info for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub server_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    #[serde(default)]
    pub name: String,      // filename
    pub data: String,      // base64 encoded
    pub mime_type: String, // e.g., "image/png", "audio/mp3"
    #[serde(default)]
    pub size: usize,       // size in bytes
}
