//! Songbird voice receive handler — captures Discord audio and pipes to daemon STT.
//!
//! Songbird is configured to decode to mono i16 16kHz, which matches the daemon
//! STT format directly — no resampling needed.

use std::sync::Arc;

use async_trait::async_trait;
use serenity::model::id::{ChannelId, GuildId};
use simply_daemon::api::Daemon;
use simply_voice::{AudioChunk, VoiceEvent, VoiceInput};
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler};
use tokio::sync::{mpsc, Mutex};

use super::VoiceMode;

/// Songbird event handler that receives decoded audio and sends to daemon STT.
pub struct VoiceReceiver {
    stt_input: mpsc::Sender<VoiceInput>,
}

impl VoiceReceiver {
    pub fn new(stt_input: mpsc::Sender<VoiceInput>) -> Self {
        Self { stt_input }
    }
}

#[async_trait]
impl VoiceEventHandler for VoiceReceiver {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::VoiceTick(tick) = ctx {
            // Collect decoded mono i16 16kHz audio from all speaking users
            let mut combined: Vec<i16> = Vec::new();

            for (_ssrc, data) in &tick.speaking {
                if let Some(decoded) = data.decoded_voice.as_ref() {
                    if combined.is_empty() {
                        combined.extend_from_slice(decoded);
                    } else {
                        // Mix multiple speakers
                        for (i, &sample) in decoded.iter().enumerate() {
                            if i < combined.len() {
                                combined[i] = combined[i].saturating_add(sample);
                            }
                        }
                    }
                }
            }

            if !combined.is_empty() {
                // Already mono 16kHz i16 — convert to PCM16 LE bytes
                let pcm16: Vec<u8> = combined.iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();
                let chunk = AudioChunk::new(pcm16);
                // Use try_send to never block the songbird event loop
                if self.stt_input.try_send(VoiceInput::Audio(chunk)).is_err() {
                    tracing::trace!("STT channel full, dropping audio chunk");
                }
            }
        }
        None
    }
}

/// Spawn a task that processes STT events and acts on them.
pub fn spawn_event_handler(
    guild_id: GuildId,
    text_channel: ChannelId,
    mode: VoiceMode,
    mut stt_events: mpsc::Receiver<VoiceEvent>,
    http: Arc<serenity::http::Http>,
    daemon: Arc<dyn Daemon>,
    call: Arc<Mutex<songbird::Call>>,
) {
    tokio::spawn(async move {
        tracing::info!(guild_id = %guild_id, mode = ?mode, "voice event handler started");

        while let Some(event) = stt_events.recv().await {
            match event {
                VoiceEvent::UserTranscript(text) => {
                    tracing::info!(guild_id = %guild_id, text = %text, "voice transcript");

                    match mode {
                        VoiceMode::Transcribe => {
                            // Post transcript to text channel
                            let _ = text_channel.say(&http, &text).await;
                        }
                        VoiceMode::Listen => {
                            // Post transcript to text channel for visibility
                            let _ = text_channel.say(&http, format!("> {text}")).await;

                            // TODO: send to daemon session, get response, TTS it back
                            // This needs access to the VoiceSession's daemon_session
                            // which is held by VoiceManager. For now, just echo.
                        }
                    }
                }
                VoiceEvent::Listening => {
                    tracing::debug!(guild_id = %guild_id, "voice: listening");
                }
                VoiceEvent::Transcribing => {
                    tracing::debug!(guild_id = %guild_id, "voice: transcribing");
                }
                VoiceEvent::Error(e) => {
                    tracing::error!(guild_id = %guild_id, error = %e, "voice STT error");
                    let _ = text_channel.say(&http, format!("Voice error: {e}")).await;
                }
                _ => {}
            }
        }

        tracing::info!(guild_id = %guild_id, "voice event handler stopped");
    });
}
