//! Voice module — manages Discord voice connections, audio pipelines,
//! and integration with the daemon's STT/TTS services.
//!
//! Flow:
//!   Songbird VoiceTick (i16 stereo 48kHz)
//!   → downsample to mono 16kHz PCM16
//!   → daemon STT stream (VoiceInput::Audio)
//!   → receive VoiceEvent::UserTranscript
//!   → post to text channel (transcribe) or send to session + TTS (listen)

mod receiver;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use serenity::model::id::{ChannelId, GuildId};
use simply_daemon_api::{
    CreateSessionOptions, Daemon, Persistence, SeedMessage, VoiceApi,
};
use simply_rpc::RequestContext;
use songbird::{Call, Songbird};
use tokio::sync::Mutex;

/// System prompt used when an agent joins voice in Listen mode.
/// Kept in a separate markdown file so it can be edited without recompiling
/// the command layout.
const VOICE_LISTEN_SYSTEM_PROMPT: &str = include_str!("system_prompt.md");

/// Result of a `VoiceManager::join_voice` call — tells the caller whether we
/// actually joined fresh or reused an existing session.
pub enum JoinOutcome {
    Joined,
    AlreadyJoined { voice_channel: ChannelId },
}

/// Active voice session for a guild.
pub struct VoiceSession {
    /// The text channel where transcripts are posted.
    pub text_channel: ChannelId,
    /// The voice channel we're connected to.
    pub voice_channel: ChannelId,
    /// Session mode.
    pub mode: VoiceMode,
    /// Daemon conversation session (for listen mode — persistent across utterances).
    pub daemon_session: Option<simply_daemon::DaemonSession>,
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
    pub(crate) sessions: Mutex<HashMap<GuildId, VoiceSession>>,
    daemon: Arc<dyn Daemon>,
    config: Mutex<config::VoiceConfig>,
    /// Songbird handle — set once the serenity client is built.
    songbird: OnceLock<Arc<Songbird>>,
}

impl VoiceManager {
    pub fn new(daemon: Arc<dyn Daemon>, voice_config: config::VoiceConfig) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            daemon,
            config: Mutex::new(voice_config),
            songbird: OnceLock::new(),
        }
    }

    pub fn daemon(&self) -> &Arc<dyn Daemon> {
        &self.daemon
    }

    /// Inject the songbird manager. Call once after the serenity client is built.
    pub fn set_songbird(&self, songbird: Arc<Songbird>) {
        let _ = self.songbird.set(songbird);
    }

    /// Stop the session and disconnect from the guild's voice channel.
    /// Returns true if we were connected and left; false otherwise.
    pub async fn leave_voice(&self, guild_id: GuildId) -> anyhow::Result<bool> {
        self.stop_session(&guild_id).await;
        let Some(songbird) = self.songbird.get() else {
            anyhow::bail!("Songbird not initialized");
        };
        match songbird.leave(guild_id).await {
            Ok(()) => Ok(true),
            Err(songbird::error::JoinError::NoCall) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn stt_provider_id(&self) -> Option<String> {
        self.config.lock().await.stt_provider.clone()
    }

    pub async fn tts_provider_id(&self) -> Option<String> {
        self.config.lock().await.tts_provider.clone()
    }

    pub async fn tts_voice_id(&self) -> Option<String> {
        self.config.lock().await.tts_voice.clone()
    }

    pub async fn set_stt_provider(&self, id: String) {
        self.config.lock().await.stt_provider = Some(id);
    }

    pub async fn set_tts_provider(&self, id: String) {
        let mut config = self.config.lock().await;
        config.tts_provider = Some(id);
        // Reset voice when provider changes — voice IDs are provider-specific
        config.tts_voice = None;
    }

    pub async fn set_tts_voice(&self, id: String) {
        self.config.lock().await.tts_voice = Some(id);
    }

    pub async fn save_config(&self, lumina_cfg: &mut config::LuminaConfig) -> Result<(), String> {
        lumina_cfg.voice = self.config.lock().await.clone();
        lumina_cfg.save()
    }

    /// Whether there's an active voice session in this guild.
    pub async fn has_session(&self, guild_id: &GuildId) -> bool {
        self.sessions.lock().await.contains_key(guild_id)
    }

    /// Connect to a guild's voice channel and start a Listen-mode conversation.
    ///
    /// Idempotent: if there's already a Listen session in this guild, returns
    /// `AlreadyJoined` without touching songbird or creating a new daemon
    /// session. Otherwise connects via songbird, creates a daemon session
    /// (seeded with `seed` and the shared voice system prompt), and wires up
    /// the STT/LLM/TTS pipeline via `start_session`. Used by both
    /// `/voice join` and the `join_voice` skill tool.
    pub async fn join_voice(
        self: &Arc<Self>,
        base_ctx: RequestContext,
        guild_id: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
        seed: Vec<SeedMessage>,
        http: Arc<serenity::http::Http>,
    ) -> anyhow::Result<JoinOutcome> {
        // Already in a voice session for this guild — don't create a second one.
        if let Some(existing) = self.sessions.lock().await.get(&guild_id) {
            tracing::info!(
                guild_id = %guild_id,
                existing_channel = %existing.voice_channel,
                "voice_manager: already in voice, reusing session"
            );
            return Ok(JoinOutcome::AlreadyJoined { voice_channel: existing.voice_channel });
        }

        let songbird = self.songbird.get()
            .ok_or_else(|| anyhow::anyhow!("Songbird not initialized"))?;
        let call = songbird.join(guild_id, voice_channel).await?;

        // Stamp the Discord origin onto the session ctx so skill tools called
        // by the voice agent know "where the conversation is happening".
        let session_ctx = base_ctx
            .with_metadata("discord.guild_id", guild_id.get().to_string())
            .with_metadata("discord.channel_id", text_channel.get().to_string())
            .with_metadata("discord.voice_channel_id", voice_channel.get().to_string());

        let session = simply_daemon::DaemonSession::create(
            self.daemon.clone(),
            session_ctx,
            CreateSessionOptions {
                persistence: Some(Persistence::Ephemeral),
                system_prompt: Some(VOICE_LISTEN_SYSTEM_PROMPT.to_string()),
                model_id: None,
                seed,
            },
        ).await?;
        tracing::info!(session_id = %session.id(), guild_id = %guild_id, "voice listen session created");

        self.start_session(
            guild_id, voice_channel, text_channel,
            VoiceMode::Listen, Some(session),
            call, http,
        ).await?;
        Ok(JoinOutcome::Joined)
    }

    /// Start a voice session and register the audio receive handler on the songbird Call.
    pub async fn start_session(
        self: &Arc<Self>,
        guild_id: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
        mode: VoiceMode,
        daemon_session: Option<simply_daemon::DaemonSession>,
        call: Arc<Mutex<Call>>,
        http: Arc<serenity::http::Http>,
    ) -> anyhow::Result<()> {
        // Connect to daemon STT — use configured provider or first available
        let stt_provider_id = match self.stt_provider_id().await {
            Some(id) => id,
            None => {
                let providers = self.daemon.voice().list_voice_providers().await?;
                providers.iter()
                    .find(|p| p.capabilities.contains(&"stt".to_string()))
                    .map(|p| p.id.clone())
                    .ok_or_else(|| anyhow::anyhow!("No STT provider available"))?
            }
        };

        let stt_handle = self.daemon.voice().voice_connect(&stt_provider_id).await?;
        let (stt_input, stt_events) = stt_handle.into_parts();

        // Register songbird receive handler — pipes audio to daemon STT
        {
            let mut handler = call.lock().await;
            handler.add_global_event(
                songbird::CoreEvent::VoiceTick.into(),
                receiver::VoiceReceiver::new(stt_input),
            );
        }

        // Spawn event handler — processes STT results
        receiver::spawn_event_handler(
            guild_id,
            text_channel,
            mode,
            stt_events,
            http,
            Arc::clone(self),
            call.clone(),
        );

        let session = VoiceSession {
            text_channel,
            voice_channel,
            mode,
            daemon_session,
        };
        self.sessions.lock().await.insert(guild_id, session);
        tracing::info!(
            guild_id = %guild_id,
            voice_channel = %voice_channel,
            text_channel = %text_channel,
            mode = ?mode,
            stt_provider = %stt_provider_id,
            "voice session started"
        );
        Ok(())
    }

    /// Stop and remove the voice session for a guild.
    pub async fn stop_session(&self, guild_id: &GuildId) -> Option<VoiceSession> {
        let session = self.sessions.lock().await.remove(guild_id);
        if session.is_some() {
            tracing::info!(guild_id = %guild_id, "voice session stopped");
        }
        session
    }

    /// Get the active mode for a guild.
    pub async fn get_mode(&self, guild_id: &GuildId) -> Option<VoiceMode> {
        self.sessions.lock().await.get(guild_id).map(|s| s.mode)
    }

    /// Synthesize text and return audio ready for songbird (interleaved stereo f32 48kHz).
    pub async fn synthesize_for_discord(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        tracing::info!(text_len = text.len(), "TTS: starting synthesis");

        // Use configured TTS provider or first available
        let tts_provider_id = match self.tts_provider_id().await {
            Some(id) => id,
            None => {
                tracing::debug!("TTS: no configured provider, fetching list");
                let providers = self.daemon.voice().list_voice_providers().await?;
                tracing::debug!(count = providers.len(), "TTS: got providers");
                providers.iter()
                    .find(|p| p.capabilities.contains(&"tts".to_string()))
                    .map(|p| p.id.clone())
                    .ok_or_else(|| anyhow::anyhow!("No TTS provider available. Use /voice provider to set one."))?
            }
        };

        // Use configured voice or first available
        let voice_id = match self.tts_voice_id().await {
            Some(id) => id,
            None => {
                tracing::debug!(provider = %tts_provider_id, "TTS: no configured voice, fetching list");
                match self.daemon.voice().list_voices(&tts_provider_id).await {
                    Ok(voices) if !voices.is_empty() => {
                        use rand::Rng;
                        let idx = rand::rng().random_range(0..voices.len());
                        tracing::info!(count = voices.len(), picked = %voices[idx].name, "TTS: picked random voice");
                        voices[idx].id.clone()
                    }
                    _ => String::new(),
                }
            }
        };

        tracing::info!(provider = %tts_provider_id, voice = %voice_id, "TTS: calling synthesize");
        let audio = self.daemon.voice().synthesize(text, &tts_provider_id, &voice_id).await?;
        let mono = audio.to_f32_samples();

        tracing::info!(
            samples = mono.len(),
            source_rate = audio.format.sample_rate,
            tts_provider = %tts_provider_id,
            voice = %voice_id,
            "TTS synthesized for Discord"
        );

        Ok(resample_mono_to_stereo_48k(&mono, audio.format.sample_rate))
    }

    /// Play TTS audio in the voice channel.
    pub async fn play_tts(&self, call: &Arc<Mutex<Call>>, text: &str) -> anyhow::Result<()> {
        let stereo = self.synthesize_for_discord(text).await?;
        let bytes: Vec<u8> = stereo.iter().flat_map(|s| s.to_le_bytes()).collect();
        let input = songbird::input::RawAdapter::new(
            std::io::Cursor::new(bytes),
            48_000,
            2,
        );
        let mut handler = call.lock().await;
        handler.play_input(input.into());
        Ok(())
    }
}

/// TypeMap key for the VoiceManager.
pub struct VoiceManagerKey;

impl serenity::prelude::TypeMapKey for VoiceManagerKey {
    type Value = Arc<VoiceManager>;
}

/// Build a WAV file in memory from f32 interleaved samples.
pub(crate) fn build_wav_f32(samples: &[f32], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bits_per_sample: u16 = 32;
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_len = (samples.len() * 4) as u32;

    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    buf
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
