//! DaemonApi — the core traits that all daemon consumers depend on.
//!
//! Split into focused traits:
//! - [`SessionApi`] — session lifecycle, conversation, events
//! - [`AssetApi`] — binary content upload
//! - [`McpApi`] — MCP service registration and tool discovery
//! - [`VoiceApi`] — voice pipeline
//!
//! See [CORE_SERVICE.md] for the full protocol specification.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use simply_core::storage::ids::AssetId;
use simply_core::storage::session::ResolvedMessage;
use simply_core::storage::InputContent;
use simply_core::ToolConfig;
use tokio::sync::{broadcast, mpsc};

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
// Session types
// ---------------------------------------------------------------------------

/// How a session should persist its data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Persistence {
    /// In-memory only — lost on daemon restart. Lumina default.
    Ephemeral,
    /// Backed by UCM storage. Noema default.
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

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

/// A user message sent to a session.
///
/// Content uses `InputContent` from simply-core — supports inline text/images/audio
/// (stored in UCM by the daemon) and asset refs (pre-uploaded via `AssetApi`).
#[derive(Debug, Clone)]
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
    pub fn into_tool_config(self) -> ToolConfig {
        ToolConfig {
            enabled: true,
            server_ids: self.server_ids,
            tool_names: self.tool_names,
        }
    }
}

/// Context seed message — replay history into a session.
///
/// Seeds can have any role — Lumina replays both user and assistant messages
/// from Discord history so the daemon has full conversational context.
#[derive(Debug, Clone)]
pub struct SeedMessage {
    pub role: llm::Role,
    pub content: Vec<InputContent>,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Events streamed from the daemon to session subscribers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonEvent {
    /// Session is ready.
    SessionReady { session_id: SessionId },
    /// Partial text from the assistant (streaming).
    TextDelta(String),
    /// Content block from the assistant.
    AssistantContent(llm::ContentBlock),
    /// The agent is calling a tool.
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Tool call completed.
    ToolResult {
        id: String,
        result: serde_json::Value,
    },
    /// Turn is complete.
    TurnComplete,
    /// Inbound event notification.
    EventNotification(InboundEvent),
    /// Error.
    Error(String),
}

/// An event pushed into the daemon (trigger interface).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

// ---------------------------------------------------------------------------
// MCP types
// ---------------------------------------------------------------------------

/// MCP service registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRegistration {
    pub name: String,
    pub endpoint: String,
}

// ---------------------------------------------------------------------------
// Voice types
// ---------------------------------------------------------------------------

/// A frame of audio data (PCM f32, mono, 16kHz by convention).
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
}

/// Voice session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoiceState {
    Inactive,
    Listening,
    Processing,
    Speaking,
}

/// Events from the daemon's voice pipeline.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    Transcription(String),
    AudioOut(AudioFrame),
    StateChanged(VoiceState),
}

/// Handle returned by `voice_connect`. Drop to disconnect.
pub struct VoiceHandle {
    pub audio_in: mpsc::Sender<AudioFrame>,
    pub events: mpsc::Receiver<VoiceEvent>,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Session lifecycle, conversation, and event streaming.
#[async_trait]
pub trait SessionApi: Send + Sync {
    /// Create a new session. Returns the ID, info, and an event stream.
    async fn create_session(
        &self,
        options: CreateSessionOptions,
    ) -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)>;

    /// Resume an existing session from storage. Returns info and an event stream.
    async fn resume_session(
        &self,
        session_id: &SessionId,
    ) -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)>;

    /// Subscribe to an already-open session's events (multiple listeners).
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

/// Binary asset upload.
#[async_trait]
pub trait AssetApi: Send + Sync {
    /// Upload binary content. Returns an `AssetId` for use in `InputContent::AssetRef`.
    async fn store_asset(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<AssetId>;
}

/// MCP service registration and tool discovery.
#[async_trait]
pub trait McpApi: Send + Sync {
    /// Register an MCP service. Tools become globally available.
    async fn register_mcp(&self, registration: McpRegistration) -> anyhow::Result<()>;

    /// Unregister an MCP service.
    async fn unregister_mcp(&self, name: &str) -> anyhow::Result<()>;

    /// List all registered MCP tool servers.
    async fn list_tools(&self) -> anyhow::Result<Vec<String>>;
}

/// Model listing and management.
#[async_trait]
pub trait ModelApi: Send + Sync {
    /// List available models from all providers.
    async fn list_models(&self) -> anyhow::Result<Vec<llm::ModelInfo>>;

    /// Get the current default model ID.
    async fn default_model_id(&self) -> String;

    /// Set the default model for new sessions.
    async fn set_default_model(&self, model_id: &str) -> anyhow::Result<()>;
}

/// Voice pipeline.
#[async_trait]
pub trait VoiceApi: Send + Sync {
    /// Connect a voice stream to a session.
    ///
    /// Client handles platform audio (CPAL/songbird/WebRTC),
    /// daemon handles STT/LLM/TTS. Drop the handle to disconnect.
    async fn voice_connect(&self, session_id: &SessionId) -> anyhow::Result<VoiceHandle>;
}

/// Convenience super-trait combining all daemon APIs.
pub trait DaemonApi: SessionApi + AssetApi + McpApi + ModelApi + VoiceApi {}

impl<T: SessionApi + AssetApi + McpApi + ModelApi + VoiceApi> DaemonApi for T {}
