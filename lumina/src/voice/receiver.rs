//! Songbird voice receive handler — captures Discord audio and pipes to daemon STT.
//!
//! Songbird is configured to decode to mono i16 16kHz, which matches the daemon
//! STT format directly — no resampling needed.

use std::sync::Arc;

use async_trait::async_trait;
use serenity::model::id::{ChannelId, GuildId};
use simply_daemon_api::Daemon;
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

/// Truncate a string to `max_chars` characters, respecting char boundaries.
fn truncate_preview(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push_str("…");
        out
    }
}

/// Spawn a task that processes STT events and acts on them.
pub fn spawn_event_handler(
    guild_id: GuildId,
    text_channel: ChannelId,
    mode: VoiceMode,
    mut stt_events: mpsc::Receiver<VoiceEvent>,
    http: Arc<serenity::http::Http>,
    voice_mgr: Arc<super::VoiceManager>,
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
                            let _ = text_channel.say(&http, format!("🗣️ {text}")).await;
                        }
                        VoiceMode::Listen => {
                            // Post transcript to text channel
                            let _ = text_channel.say(&http, format!("🗣️ {text}")).await;

                            // Send to daemon session → get LLM response → TTS → play
                            let response = {
                                let mut sessions = voice_mgr.sessions.lock().await;
                                if let Some(session) = sessions.get_mut(&guild_id) {
                                    if let Some(ref mut daemon_session) = session.daemon_session {
                                        // Send user transcript to session
                                        let send_result = daemon_session.send(simply_daemon_api::UserMessage {
                                            content: vec![simply_daemon_api::InputContent::Text {
                                                text: text.clone(),
                                            }],
                                        }).await;

                                        if let Err(e) = send_result {
                                            tracing::error!(error = %e, "failed to send to session");
                                            None
                                        } else {
                                            // Collect the response, surfacing tool activity to the text channel.
                                            let mut response_text = String::new();
                                            loop {
                                                match daemon_session.recv().await {
                                                    Ok(simply_daemon_api::DaemonEvent::TextDelta(delta)) => {
                                                        response_text.push_str(&delta);
                                                    }
                                                    Ok(simply_daemon_api::DaemonEvent::ToolCall { name, arguments, .. }) => {
                                                        tracing::info!(tool = %name, "voice tool call");
                                                        let args_preview = serde_json::to_string(&arguments)
                                                            .unwrap_or_default();
                                                        let args_preview = truncate_preview(&args_preview, 200);
                                                        let _ = text_channel.say(&http, format!("🔧 `{name}` {args_preview}")).await;
                                                    }
                                                    Ok(simply_daemon_api::DaemonEvent::ToolResult { result, .. }) => {
                                                        let result_preview = serde_json::to_string(&result)
                                                            .unwrap_or_default();
                                                        let result_preview = truncate_preview(&result_preview, 400);
                                                        let _ = text_channel.say(&http, format!("✅ {result_preview}")).await;
                                                    }
                                                    Ok(simply_daemon_api::DaemonEvent::TurnComplete) => break,
                                                    Ok(simply_daemon_api::DaemonEvent::Error(e)) => {
                                                        tracing::error!(error = %e, "session error");
                                                        break;
                                                    }
                                                    Err(_) => break,
                                                    _ => {}
                                                }
                                            }
                                            if response_text.trim().is_empty() { None } else { Some(response_text) }
                                        }
                                    } else { None }
                                } else { None }
                            };

                            if let Some(response_text) = response {
                                // TTS → play in voice channel (with retry + fallback)
                                let tts_ok = match voice_mgr.synthesize_for_discord(&response_text).await {
                                    Ok(stereo) => {
                                        let wav = super::build_wav_f32(&stereo, 48_000, 2);
                                        let cursor = std::io::Cursor::new(wav);
                                        let input = songbird::input::Input::Live(
                                            songbird::input::LiveInput::Raw(
                                                songbird::input::AudioStream { input: Box::new(cursor) },
                                            ),
                                            None,
                                        );
                                        let mut handler = call.lock().await;
                                        handler.play_input(input);
                                        tracing::info!("TTS response playing in voice channel");
                                        true
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "TTS failed, falling back to text");
                                        false
                                    }
                                };

                                // Always post text — as primary if TTS failed, as supplement if TTS worked
                                if tts_ok {
                                    let _ = text_channel.say(&http, format!("💬 {response_text}")).await;
                                } else {
                                    let _ = text_channel.say(&http, format!("🔇 {response_text}")).await;
                                }
                            }
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
