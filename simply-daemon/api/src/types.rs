//! API types — re-exported simply-core/llm types + daemon-specific types.
//!
//! This is the canonical import point for consumers of the daemon API.

use serde::{Deserialize, Serialize};
#[cfg(feature = "ts")]
use ts_rs::TS;

// ---------------------------------------------------------------------------
// Re-exported IDs
// ---------------------------------------------------------------------------

pub use simply_core::storage::ids::{
    AssetId, ConversationId, DocumentId, EntityId, RevisionId, SpanId, TabId, TurnId, UserId,
};

// ---------------------------------------------------------------------------
// Re-exported storage types (API-facing only)
// ---------------------------------------------------------------------------

pub use simply_core::storage::{
    InputContent, ResolvedContent, ResolvedMessage,
    DocumentSource,
};
pub use simply_core::storage::types::BlobHash;

// ---------------------------------------------------------------------------
// Re-exported LLM types
// ---------------------------------------------------------------------------

pub use llm::{
    ChatMessage, ContentBlock, ModelCapability, ModelInfo, ProviderInfo, Role, ToolResultContent,
    list_providers,
};

// ---------------------------------------------------------------------------
// Re-exported MCP config types
// ---------------------------------------------------------------------------

pub use simply_core::{AuthMethod, McpRegistry, ServerConfig};

// ---------------------------------------------------------------------------
// Daemon-specific types
// ---------------------------------------------------------------------------

/// Opaque session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "admin/src/lib/generated/types/"))]
pub struct SessionId(String);

impl SessionId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Events streamed from the daemon to session subscribers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "admin/src/lib/generated/types/"))]
pub enum DaemonEvent {
    SessionReady { session_id: SessionId },
    UserMessage(ChatMessage),
    TextDelta(String),
    AssistantContent(ContentBlock),
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        id: String,
        result: serde_json::Value,
    },
    TurnComplete,
    EventNotification(InboundEvent),
    Error(String),
}

/// An event pushed into the daemon (trigger interface).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "admin/src/lib/generated/types/"))]
#[serde(rename_all = "camelCase")]
pub struct InboundEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

/// Information about a stored conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "admin/src/lib/generated/types/"))]
#[serde(rename_all = "camelCase")]
pub struct ConversationInfo {
    pub id: ConversationId,
    pub name: Option<String>,
    pub message_count: usize,
    pub created_at: i64,
}

/// Status of the embedding queue.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "admin/src/lib/generated/types/"))]
pub struct EmbeddingQueueStatus {
    pub pending: usize,
    pub processing: usize,
    pub completed: u64,
    pub failed: u64,
}
