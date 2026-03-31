//! Shared types — both daemon API types and re-exported simply-core/llm types.
//!
//! Clients (Noema, Lumina) should import from here or `simply_daemon::types`,
//! not from simply-core or llm directly.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Re-exported IDs
// ---------------------------------------------------------------------------

pub use simply_core::storage::ids::{
    AssetId, ConversationId, DocumentId, EntityId, RevisionId, SpanId, TabId, TurnId, UserId,
};

// ---------------------------------------------------------------------------
// Re-exported storage types
// ---------------------------------------------------------------------------

pub use simply_core::storage::{
    InputContent, ResolvedContent, ResolvedMessage,
    Entity, EntityType,
    Document, DocumentSource, DocumentTab, StoredEditable,
    DocumentStore, Stores, UserStore,
    FsBlobStore, SqliteStore,
};
pub use simply_core::storage::types::BlobHash;
pub use simply_core::storage::coordinator::StorageCoordinator;
pub use simply_core::storage::traits::StorageTypes;

// ---------------------------------------------------------------------------
// Re-exported LLM types
// ---------------------------------------------------------------------------

pub use llm::{ChatMessage, ContentBlock, ModelCapability, ModelInfo, Role, ToolResultContent};

// ---------------------------------------------------------------------------
// Re-exported MCP config types
// ---------------------------------------------------------------------------

pub use simply_core::{AuthMethod, McpRegistry, ServerConfig};
pub use simply_core::mcp::{ServerStatus, spawn_retry_task, start_auto_connect};

// ---------------------------------------------------------------------------
// Daemon-specific types
// ---------------------------------------------------------------------------

/// Opaque session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

/// Information about a stored conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInfo {
    pub id: ConversationId,
    pub name: Option<String>,
    pub message_count: usize,
    pub created_at: i64,
}
