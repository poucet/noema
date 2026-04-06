//! Voice module — manages Discord voice connections, audio pipelines,
//! and integration with the daemon's STT/TTS services.
//!
//! The voice module owns long-lived connections:
//! - Songbird voice channel connections (audio receive/send)
//! - Daemon STT streams (audio → transcription)
//! - TTS synthesis (text → audio → voice channel)
//!
//! Commands in `commands/voice.rs` are thin wrappers that call into this module.

use std::collections::HashMap;
use std::sync::Arc;

use serenity::model::id::{ChannelId, GuildId};
use simply_daemon::api::Daemon;
use tokio::sync::Mutex;

/// Active voice session for a guild.
pub struct VoiceSession {
    /// The text channel where transcripts are posted.
    pub text_channel: ChannelId,
    /// The voice channel we're connected to.
    pub voice_channel: ChannelId,
    /// Session mode.
    pub mode: VoiceMode,
}

/// What the voice session is doing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VoiceMode {
    /// Transcribe-only: post speech as text messages.
    Transcribe,
    /// Full conversation: STT → LLM → TTS response.
    Listen,
}

/// Manages voice sessions across guilds.
pub struct VoiceManager {
    sessions: Mutex<HashMap<GuildId, VoiceSession>>,
    daemon: Arc<dyn Daemon>,
}

impl VoiceManager {
    pub fn new(daemon: Arc<dyn Daemon>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            daemon,
        }
    }

    /// Start a voice session in the given guild.
    pub async fn start_session(
        &self,
        guild_id: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
        mode: VoiceMode,
    ) {
        let session = VoiceSession {
            text_channel,
            voice_channel,
            mode,
        };
        self.sessions.lock().await.insert(guild_id, session);
        tracing::info!(
            guild_id = %guild_id,
            voice_channel = %voice_channel,
            text_channel = %text_channel,
            mode = ?mode,
            "voice session started"
        );
    }

    /// Stop and remove the voice session for a guild.
    pub async fn stop_session(&self, guild_id: &GuildId) -> Option<VoiceSession> {
        let session = self.sessions.lock().await.remove(guild_id);
        if session.is_some() {
            tracing::info!(guild_id = %guild_id, "voice session stopped");
        }
        session
    }

    /// Get the active session for a guild.
    pub async fn get_session(&self, guild_id: &GuildId) -> Option<VoiceMode> {
        self.sessions.lock().await.get(guild_id).map(|s| s.mode)
    }

    /// Synthesize text and return audio ready for songbird (stereo f32 48kHz).
    pub async fn synthesize_for_discord(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        use simply_daemon::api::VoiceApi;

        let providers = self.daemon.voice().list_voice_providers().await?;
        let tts_provider = providers.iter()
            .find(|p| p.capabilities.contains(&"tts".to_string()))
            .ok_or_else(|| anyhow::anyhow!("No TTS provider available"))?;

        let audio = self.daemon.voice().synthesize(text, &tts_provider.id, "").await?;
        let mono = audio.to_f32_samples();

        // Resample to stereo 48kHz for Discord
        Ok(resample_mono_to_stereo_48k(&mono, audio.format.sample_rate))
    }
}

/// TypeMap key for the VoiceManager.
pub struct VoiceManagerKey;

impl serenity::prelude::TypeMapKey for VoiceManagerKey {
    type Value = Arc<VoiceManager>;
}

/// Resample mono audio to interleaved stereo 48kHz.
fn resample_mono_to_stereo_48k(mono: &[f32], source_rate: u32) -> Vec<f32> {
    let ratio = 48_000.0 / source_rate as f64;
    let output_len = (mono.len() as f64 * ratio) as usize;
    let mut stereo = Vec::with_capacity(output_len * 2);

    for i in 0..output_len {
        let src_idx = (i as f64 / ratio) as usize;
        let sample = mono.get(src_idx).copied().unwrap_or(0.0);
        stereo.push(sample);
        stereo.push(sample);
    }

    stereo
}
