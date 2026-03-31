//! Shared types used across multiple daemon API traits.

use serde::{Deserialize, Serialize};
use crate::types::ConversationId;

// ---------------------------------------------------------------------------
// Identifiers
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

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Events streamed from the daemon to session subscribers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonEvent {
    SessionReady { session_id: SessionId },
    UserMessage(llm::ChatMessage),
    TextDelta(String),
    AssistantContent(llm::ContentBlock),
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

// ---------------------------------------------------------------------------
// Conversation info (used by ConversationApi)
// ---------------------------------------------------------------------------

/// Information about a stored conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInfo {
    pub id: ConversationId,
    pub name: Option<String>,
    pub message_count: usize,
    pub created_at: i64,
}
