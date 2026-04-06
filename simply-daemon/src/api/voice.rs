//! Voice pipeline.

use async_trait::async_trait;
use tokio::sync::mpsc;

use simply_voice::{AudioChunk, VoiceEvent, VoiceState};

use super::types::SessionId;

/// Handle returned by `voice_connect`. Drop to disconnect.
pub struct VoiceHandle {
    pub audio_in: mpsc::Sender<AudioChunk>,
    pub events: mpsc::Receiver<VoiceEvent>,
}

#[simply_rpc::rpc_service("voice")]
#[async_trait]
pub trait VoiceApi: Send + Sync {
    /// Connect a voice stream to a session.
    ///
    /// Client handles platform audio (CPAL/songbird/WebRTC),
    /// daemon handles VAD/STT/LLM/TTS via simply-voice providers.
    #[rpc(skip)]
    async fn voice_connect(&self, session_id: &SessionId) -> anyhow::Result<VoiceHandle>;

    /// Disconnect a voice stream from a session.
    #[rpc(delete = "/voice/{session_id}")]
    async fn voice_disconnect(&self, session_id: &SessionId) -> anyhow::Result<()>;
}
