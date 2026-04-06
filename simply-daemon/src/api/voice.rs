//! Voice pipeline.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use simply_voice::{AudioChunk, VoiceEvent, VoiceState};

use super::types::SessionId;

/// Handle returned by `voice_connect`. Drop to disconnect.
pub struct VoiceHandle {
    pub audio_in: mpsc::Sender<AudioChunk>,
    pub events: mpsc::Receiver<VoiceEvent>,
}

/// Info about an available voice provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProviderInfo {
    /// Unique identifier (e.g. "whisper", "voxtral", "gemini").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// What this provider supports.
    pub capabilities: Vec<String>,
}

#[simply_rpc::rpc_service("voice")]
#[async_trait]
pub trait VoiceApi: Send + Sync {
    /// List available voice providers.
    #[rpc(get = "/voice/provider")]
    async fn list_voice_providers(&self) -> anyhow::Result<Vec<VoiceProviderInfo>>;

    /// Connect a voice stream to a session using a specific provider.
    ///
    /// Client handles platform audio (CPAL/songbird/WebRTC),
    /// daemon handles VAD/STT/LLM/TTS via simply-voice providers.
    #[rpc(skip)]
    async fn voice_connect(&self, session_id: &SessionId, provider_id: &str) -> anyhow::Result<VoiceHandle>;

    /// Disconnect a voice stream from a session.
    #[rpc(delete = "/voice/{session_id}")]
    async fn voice_disconnect(&self, session_id: &SessionId) -> anyhow::Result<()>;
}
