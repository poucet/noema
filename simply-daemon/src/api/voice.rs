//! Voice pipeline.

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::types::SessionId;

/// A frame of audio data (PCM f32, mono, 16kHz by convention).
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
}

/// Voice session state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

#[async_trait]
pub trait VoiceApi: Send + Sync {
    /// Connect a voice stream to a session.
    ///
    /// Client handles platform audio (CPAL/songbird/WebRTC),
    /// daemon handles STT/LLM/TTS. Drop the handle to disconnect.
    async fn voice_connect(&self, session_id: &SessionId) -> anyhow::Result<VoiceHandle>;
}
