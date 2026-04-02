//! Session lifecycle and event streaming.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use super::types::{DaemonEvent, InboundEvent, InputContent, ResolvedMessage, SessionId};

// ---------------------------------------------------------------------------
// Session-specific types
// ---------------------------------------------------------------------------

/// How a session should persist its data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Persistence {
    Ephemeral,
    Persistent,
}

/// Options when creating a new session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateSessionOptions {
    pub persistence: Option<Persistence>,
    pub system_prompt: Option<String>,
    pub model_id: Option<String>,
}

/// Information about a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub persistence: Persistence,
    pub model_id: String,
    pub created_at: String,
}

/// A user message sent to a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub content: Vec<InputContent>,
    pub tool_filter: Option<ToolFilter>,
}

/// Filter which tools the agent can use for a turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFilter {
    pub server_ids: Option<Vec<String>>,
    pub tool_names: Option<Vec<String>>,
}

impl ToolFilter {
    pub fn into_tool_config(self) -> simply_core::ToolConfig {
        simply_core::ToolConfig {
            enabled: true,
            server_ids: self.server_ids,
            tool_names: self.tool_names,
        }
    }
}

/// Context seed message — replay history into a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedMessage {
    pub role: llm::Role,
    pub content: Vec<InputContent>,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

#[simply_rpc::rpc_service("session")]
#[async_trait]
pub trait SessionApi: Send + Sync {
    /// Create a new session. Returns info and an event stream.
    #[rpc(stream)]
    async fn create_session(
        &self,
        options: CreateSessionOptions,
    ) -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)>;

    /// Resume an existing session from storage. Returns info and an event stream.
    #[rpc(stream)]
    async fn resume_session(
        &self,
        session_id: &SessionId,
    ) -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)>;

    /// Subscribe to an already-open session's events (multiple listeners).
    #[rpc(stream)]
    async fn subscribe_session(
        &self,
        session_id: &SessionId,
    ) -> anyhow::Result<broadcast::Receiver<DaemonEvent>>;

    /// Close a session and free its memory.
    async fn close_session(&self, session_id: &SessionId) -> anyhow::Result<()>;

    /// Close all sessions (client disconnect cleanup).
    async fn close_all_sessions(&self) -> anyhow::Result<()>;

    /// Replay context into a session (e.g., Discord history).
    async fn seed_context(
        &self,
        session_id: &SessionId,
        messages: Vec<SeedMessage>,
    ) -> anyhow::Result<()>;

    /// List active sessions.
    async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>>;

    /// Change persistence mode.
    async fn set_persistence(
        &self,
        session_id: &SessionId,
        persistence: Persistence,
    ) -> anyhow::Result<()>;

    /// Send a user message. Events arrive on the session's broadcast receiver.
    async fn send_message(
        &self,
        session_id: &SessionId,
        message: UserMessage,
    ) -> anyhow::Result<()>;

    /// Get messages for display (resolved content with turn IDs).
    async fn get_messages(
        &self,
        session_id: &SessionId,
    ) -> anyhow::Result<Vec<ResolvedMessage>>;

    /// Change the model for a session.
    async fn set_model(&self, session_id: &SessionId, model_id: &str) -> anyhow::Result<()>;

    /// Reload messages from storage (e.g., after span selection change).
    async fn reload(&self, session_id: &SessionId) -> anyhow::Result<()>;

    /// Push an inbound event into the daemon.
    async fn push_event(&self, event: InboundEvent) -> anyhow::Result<()>;
}
