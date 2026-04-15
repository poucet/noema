//! Voice pipeline.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use simply_rpc::{RequestContext, StreamHandle};
use simply_voice::{Audio, VoiceEvent, VoiceInput};

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
    #[rpc(get = "/voice/provider", no_tool)]
    async fn list_voice_providers(&self, ctx: &RequestContext) -> anyhow::Result<Vec<VoiceProviderInfo>>;

    /// Connect a bidirectional voice stream.
    #[rpc(stream = "/voice/stream/{provider_id}", no_tool)]
    async fn voice_connect(&self, ctx: &RequestContext, provider_id: &str) -> anyhow::Result<StreamHandle<VoiceInput, VoiceEvent>>;

    /// Synthesize text to speech. Returns audio data.
    #[rpc(post = "/voice/tts", no_tool)]
    async fn synthesize(&self, ctx: &RequestContext, text: &str, provider_id: &str, voice: &str) -> anyhow::Result<Audio>;

    /// List available TTS voices for a provider.
    #[rpc(get = "/voice/tts/voices/{provider_id}", no_tool)]
    async fn list_voices(&self, ctx: &RequestContext, provider_id: &str) -> anyhow::Result<Vec<simply_voice::Voice>>;

    /// Disconnect a voice stream.
    #[rpc(delete = "/voice/{session_id}", no_tool)]
    async fn voice_disconnect(&self, ctx: &RequestContext, session_id: &str) -> anyhow::Result<()>;
}
