//! Songbird voice receive handler — captures Discord audio and pipes to daemon STT.
//!
//! Songbird is configured to decode to mono i16 16kHz, which matches the daemon
//! STT format directly — no resampling needed.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serenity::model::id::{ChannelId, GuildId};
use simply_daemon_api::{Daemon, ToolResultContent};
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

/// Post a `DaemonEvent::ToolResult.result` to the text channel using the
/// shared renderer so voice-session tool output looks the same as the
/// `/tool call` surface (embeds, JSON-aware pagination, attachments).
///
/// Falls back to wrapping non-`ToolResultContent` payloads as a single
/// text block so the renderer can still paginate them.
async fn post_tool_result(
    result: &serde_json::Value,
    tool_name: &str,
    text_channel: ChannelId,
    http: &Arc<serenity::http::Http>,
    voice_mgr: &Arc<super::VoiceManager>,
) {
    let blocks = match serde_json::from_value::<Vec<ToolResultContent>>(result.clone()) {
        Ok(b) => b,
        Err(_) => {
            let text = result.as_str().map(|s| s.to_string())
                .unwrap_or_else(|| serde_json::to_string_pretty(result).unwrap_or_default());
            vec![ToolResultContent::Text { text }]
        }
    };

    let Some(shard) = voice_mgr.shard() else {
        tracing::warn!("voice: shard not injected yet — tool result dropped");
        return;
    };
    if let Err(e) = crate::tool_render::render_tool_result_to_channel(
        http, shard, text_channel, tool_name, Ok(blocks), Vec::new(),
    ).await {
        tracing::warn!(tool = %tool_name, error = %e, "voice: failed to post tool result");
    }
}

/// Send one user transcript to the daemon session and collect the LLM response,
/// surfacing tool activity to the text channel along the way. Runs without
/// holding `voice_mgr.sessions.lock()` so skill tools that need the same lock
/// (e.g. `leave_voice`) can run concurrently.
async fn process_llm_turn(
    daemon_session: &mut simply_daemon::DaemonSession,
    user_text: &str,
    text_channel: ChannelId,
    http: &Arc<serenity::http::Http>,
    voice_mgr: &Arc<super::VoiceManager>,
) -> Option<String> {
    let send_result = daemon_session.send(simply_daemon_api::UserMessage {
        content: vec![simply_daemon_api::InputContent::Text { text: user_text.to_string() }],
    }).await;
    if let Err(e) = send_result {
        tracing::error!(error = %e, "failed to send to session");
        return None;
    }

    // DaemonEvent::ToolResult only carries the tool-call id; keep the
    // id→name mapping seen on ToolCall so we can title the result embed
    // with the tool's name.
    let mut tool_names: HashMap<String, String> = HashMap::new();
    let mut response_text = String::new();
    loop {
        match daemon_session.recv().await {
            Ok(simply_daemon_api::DaemonEvent::TextDelta(delta)) => {
                response_text.push_str(&delta);
            }
            Ok(simply_daemon_api::DaemonEvent::ToolCall { id, name, arguments }) => {
                tracing::info!(tool = %name, "voice tool call");
                let preview = truncate_preview(&serde_json::to_string(&arguments).unwrap_or_default(), 200);
                let _ = text_channel.say(http, format!("🔧 `{name}` {preview}")).await;
                tool_names.insert(id, name);
            }
            Ok(simply_daemon_api::DaemonEvent::ToolResult { id, result }) => {
                let name = tool_names.remove(&id).unwrap_or_else(|| "tool".to_string());
                post_tool_result(&result, &name, text_channel, http, voice_mgr).await;
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

/// Spawn a task that processes STT events and acts on them. Returns the
/// `JoinHandle` so the owning `VoiceSession` can abort the task on stop.
pub fn spawn_event_handler(
    guild_id: GuildId,
    text_channel: ChannelId,
    mode: VoiceMode,
    mut stt_events: mpsc::Receiver<VoiceEvent>,
    http: Arc<serenity::http::Http>,
    voice_mgr: Arc<super::VoiceManager>,
    call: Arc<Mutex<songbird::Call>>,
    tts: Option<super::TtsBinding>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!(guild_id = %guild_id, mode = ?mode, "voice event handler started");

        if let Some(binding) = tts.as_ref() {
            if let Err(e) = voice_mgr.play_tts(&call, binding, "Hi, I'm Lumina, ready to help.").await {
                tracing::warn!(error = %e, "welcome TTS failed");
            }
        }

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

                            // Take the DaemonSession out of the map so tool calls (e.g.
                            // leave_voice, which takes sessions.lock()) can run concurrently
                            // with the send/recv loop and don't deadlock on our lock.
                            let daemon_session = {
                                let mut sessions = voice_mgr.sessions.lock().await;
                                sessions.get_mut(&guild_id).and_then(|s| s.daemon_session.take())
                            };

                            let (response, daemon_session) = if let Some(mut ds) = daemon_session {
                                let response = process_llm_turn(&mut ds, &text, text_channel, &http, &voice_mgr).await;
                                (response, Some(ds))
                            } else {
                                (None, None)
                            };

                            // Put the session back — unless it was removed (e.g. by a
                            // concurrent leave_voice, in which case daemon_session drops).
                            if let Some(ds) = daemon_session {
                                let mut sessions = voice_mgr.sessions.lock().await;
                                if let Some(s) = sessions.get_mut(&guild_id) {
                                    s.daemon_session = Some(ds);
                                }
                            }

                            if let Some(response_text) = response {
                                let tts_ok = if let Some(binding) = tts.as_ref() {
                                    match voice_mgr.play_tts(&call, binding, &response_text).await {
                                        Ok(()) => {
                                            tracing::info!("TTS response playing in voice channel");
                                            true
                                        }
                                        Err(e) => {
                                            tracing::warn!(error = %e, "TTS failed, falling back to text");
                                            false
                                        }
                                    }
                                } else {
                                    false
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
                    // Barge-in: VAD says the user started speaking, so stop any
                    // TTS playback we're in the middle of. The full response is
                    // still in the text channel, the user doesn't need to wait
                    // for the audio to finish before giving a new command.
                    {
                        let mut handler = call.lock().await;
                        handler.stop();
                    }
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
    })
}
