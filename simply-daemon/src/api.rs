//! DaemonApi — the core trait that all daemon consumers depend on.
//!
//! Noema, Lumina, and any future client use this trait. Whether the daemon
//! runs in-process or as a separate service is a runtime/build decision,
//! not a code decision.
//!
//! See [CORE_SERVICE.md] for the full protocol specification.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use simply_core::storage::InputContent;
use simply_core::ToolConfig;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

/// Opaque session identifier (maps to a UCM ConversationId for persistent sessions).
pub type SessionId = String;

// ---------------------------------------------------------------------------
// Client → Daemon messages
// ---------------------------------------------------------------------------

/// How a session should persist its data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Persistence {
    /// In-memory only — lost on daemon restart. Lumina default (Discord is source of truth).
    Ephemeral,
    /// Backed by UCM storage. Noema default.
    Persistent,
}

/// Options when creating a new session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionOptions {
    /// Persistence mode. Defaults per-platform if None.
    pub persistence: Option<Persistence>,
    /// Optional system prompt to seed the session.
    pub system_prompt: Option<String>,
    /// Initial model ID (e.g., "anthropic/claude-sonnet-4-20250514").
    pub model_id: Option<String>,
}

impl Default for CreateSessionOptions {
    fn default() -> Self {
        Self {
            persistence: None,
            system_prompt: None,
            model_id: None,
        }
    }
}

/// A user message sent to a session.
///
/// Content uses `InputContent` from simply-core — supports inline text/images/audio
/// (daemon stores them in UCM) and asset refs (already stored via `upload_asset`).
#[derive(Debug, Clone)]
pub struct UserMessage {
    pub content: Vec<InputContent>,
    /// Which MCP tools to enable for this turn. None = all.
    pub tool_filter: Option<ToolFilter>,
}

/// Filter which tools the agent can use for a turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFilter {
    pub server_ids: Option<Vec<String>>,
    pub tool_names: Option<Vec<String>>,
}

impl ToolFilter {
    /// Convert to a simply-core `ToolConfig`.
    pub fn into_tool_config(self) -> ToolConfig {
        ToolConfig {
            enabled: true,
            server_ids: self.server_ids,
            tool_names: self.tool_names,
        }
    }
}

/// Context seed message — replay history into a session (e.g., Discord channel messages).
///
/// Seeds can have any role — Lumina replays both user and assistant messages
/// from Discord history so the daemon has full conversational context.
#[derive(Debug, Clone)]
pub struct SeedMessage {
    pub role: llm::Role,
    pub content: Vec<InputContent>,
}

/// MCP service registration from a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRegistration {
    /// Unique name for this MCP service.
    pub name: String,
    /// MCP endpoint URL.
    pub endpoint: String,
}

/// An event pushed into the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundEvent {
    /// Event type (e.g., "github.pr_opened", "timer.fired").
    pub event_type: String,
    /// Arbitrary event payload.
    pub payload: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Daemon → Client events
// ---------------------------------------------------------------------------

/// Events streamed back from the daemon during a conversation turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonEvent {
    /// Session was created or resumed.
    SessionReady { session_id: SessionId },
    /// Partial text from the assistant (streaming).
    TextDelta(String),
    /// Non-text content from the assistant.
    Content(InputContent),
    /// The agent wants to call a tool.
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Tool call completed with a result.
    ToolResult {
        id: String,
        result: serde_json::Value,
    },
    /// Turn is complete — includes resolved messages with turn IDs.
    TurnComplete,
    /// An intent/event notification targeting this client.
    EventNotification(InboundEvent),
    /// Something went wrong.
    Error(String),
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
// Voice types
// ---------------------------------------------------------------------------

/// A frame of audio data. PCM format details (sample rate, channels) are
/// negotiated at voice_connect time.
#[derive(Debug, Clone)]
pub struct AudioFrame {
    /// Raw PCM samples (f32, mono, 16kHz by convention).
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

/// Events from the daemon's voice pipeline back to the client.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// Daemon detected speech and transcribed it.
    Transcription(String),
    /// TTS audio to play on the client's output device.
    AudioOut(AudioFrame),
    /// Voice pipeline state changed.
    StateChanged(VoiceState),
}

/// Handle returned by `voice_connect`. Dropping it disconnects the voice session.
pub struct VoiceHandle {
    /// Send mic audio into the daemon's STT pipeline.
    pub audio_in: mpsc::Sender<AudioFrame>,
    /// Receive transcriptions and TTS audio from the daemon.
    pub events: mpsc::Receiver<VoiceEvent>,
}

// ---------------------------------------------------------------------------
// The trait
// ---------------------------------------------------------------------------

/// The core API surface of the Simply daemon.
///
/// All methods are async — both in-process and remote implementations may do I/O.
/// Clients (Noema, Lumina) depend on this trait, never on a concrete implementation.
#[async_trait]
pub trait DaemonApi: Send + Sync {
    // -- Session lifecycle ---------------------------------------------------

    /// Create a new conversation session.
    async fn create_session(&self, options: CreateSessionOptions) -> anyhow::Result<SessionId>;

    /// Resume an existing session (e.g., after Noema restart).
    /// Returns the session info if it exists, or an error if the session is gone.
    /// For persistent sessions, this reloads from UCM storage.
    /// For ephemeral sessions, the client must re-seed via `seed_context`.
    async fn resume_session(&self, session_id: &str) -> anyhow::Result<SessionInfo>;

    /// Destroy a session and free its memory. Persistent data in UCM is not deleted.
    async fn close_session(&self, session_id: &str) -> anyhow::Result<()>;

    /// Close all sessions. Called on client disconnect to prevent memory leaks.
    /// For the WebSocket case, the server calls this when a connection drops.
    async fn close_all_sessions(&self) -> anyhow::Result<()>;

    /// Replay context into a session (e.g., Lumina re-sending Discord history).
    async fn seed_context(
        &self,
        session_id: &str,
        messages: Vec<SeedMessage>,
    ) -> anyhow::Result<()>;

    /// List active sessions.
    async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>>;

    /// Change persistence mode for a session (ephemeral ↔ persistent).
    async fn set_persistence(
        &self,
        session_id: &str,
        persistence: Persistence,
    ) -> anyhow::Result<()>;

    // -- Conversation --------------------------------------------------------

    /// Send a user message. Returns a stream of DaemonEvents.
    ///
    /// The returned Vec is a simplification — a real streaming impl would use
    /// a channel or async stream. Good enough for the in-process skeleton;
    /// the remote impl will use proper streaming.
    async fn send_message(
        &self,
        session_id: &str,
        message: UserMessage,
    ) -> anyhow::Result<Vec<DaemonEvent>>;

    /// Change the model for a session.
    async fn set_model(&self, session_id: &str, model_id: &str) -> anyhow::Result<()>;

    /// Truncate conversation history to before a specific turn.
    /// Pass None to clear all history.
    async fn truncate(&self, session_id: &str, before_turn: Option<&str>) -> anyhow::Result<()>;

    // -- Assets --------------------------------------------------------------

    /// Upload binary content to the daemon's blob store.
    ///
    /// Returns an `AssetId` that can be used in `InputContent::AssetRef`
    /// when sending messages. This lets clients pre-upload large binaries
    /// instead of inlining base64 on every message.
    async fn upload_asset(
        &self,
        data: Vec<u8>,
        media_type: &str,
    ) -> anyhow::Result<simply_core::storage::ids::AssetId>;

    // -- MCP tools -----------------------------------------------------------

    /// Register an MCP service. Tools become available to all sessions globally.
    async fn register_mcp(&self, registration: McpRegistration) -> anyhow::Result<()>;

    /// Unregister an MCP service.
    async fn unregister_mcp(&self, name: &str) -> anyhow::Result<()>;

    /// List all registered MCP tools across all services.
    async fn list_tools(&self) -> anyhow::Result<Vec<String>>;

    // -- Events --------------------------------------------------------------

    /// Push an event into the daemon (trigger interface).
    async fn push_event(&self, event: InboundEvent) -> anyhow::Result<()>;

    // -- Voice ---------------------------------------------------------------

    /// Connect a voice stream to a session. Returns a handle with:
    /// - `audio_in`: send mic PCM frames into the daemon's STT pipeline
    /// - `events`: receive transcriptions, TTS audio frames, and state changes
    ///
    /// The client is responsible for platform-specific audio capture/playback
    /// (CPAL on desktop, songbird on Discord, WebRTC in browser) and converting
    /// to/from the common PCM format. The daemon handles STT, LLM, and TTS.
    ///
    /// Drop the handle to disconnect voice.
    async fn voice_connect(&self, session_id: &str) -> anyhow::Result<VoiceHandle>;
}
