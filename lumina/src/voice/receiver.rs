//! Songbird voice receive handler — captures Discord audio and pipes to daemon STT.
//!
//! Songbird is configured to decode to mono i16 16kHz, which matches the daemon
//! STT format directly — no resampling needed.

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use serenity::builder::{CreateAttachment, CreateMessage};
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

/// Post a `DaemonEvent::ToolResult.result` to the text channel, dispatched
/// by content type.
///
/// The daemon ships results as the wire-format `Vec<ToolResultContent>`
/// (MCP-style blocks). Parse that, then:
/// - Text   → one `✅ …` text message
/// - Image  → Discord file attachment (native previews)
/// - Audio  → Discord file attachment
/// Falls back to pretty-printing the raw value if it doesn't look like
/// a Content-block array.
async fn post_tool_result(
    result: &serde_json::Value,
    text_channel: &ChannelId,
    http: &Arc<serenity::http::Http>,
) {
    if let Ok(blocks) = serde_json::from_value::<Vec<ToolResultContent>>(result.clone()) {
        let mut text_parts: Vec<String> = Vec::new();
        let mut attachments: Vec<CreateAttachment> = Vec::new();

        for block in blocks {
            match block {
                ToolResultContent::Text { text } => text_parts.push(text),
                ToolResultContent::Image { data, mime_type } => {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&data) {
                        let ext = mime_type.split('/').last().unwrap_or("png");
                        attachments.push(CreateAttachment::bytes(bytes, format!("image.{ext}")));
                    }
                }
                ToolResultContent::Audio { data, mime_type } => {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&data) {
                        let ext = mime_type.split('/').last().unwrap_or("wav");
                        attachments.push(CreateAttachment::bytes(bytes, format!("audio.{ext}")));
                    }
                }
            }
        }

        if !text_parts.is_empty() {
            let body = truncate_preview(&text_parts.join("\n"), 400);
            let _ = text_channel.say(http, format!("✅ {body}")).await;
        }
        if !attachments.is_empty() {
            let mut msg = CreateMessage::new();
            for a in attachments { msg = msg.add_file(a); }
            let _ = text_channel.send_message(http, msg).await;
        }
        return;
    }

    // Not a Content-block array — fall back to stringifying the raw value.
    let body = if let Some(s) = result.as_str() {
        s.to_string()
    } else {
        serde_json::to_string_pretty(result).unwrap_or_default()
    };
    let preview = truncate_preview(&body, 400);
    let _ = text_channel.say(http, format!("✅ {preview}")).await;
}

/// Send one user transcript to the daemon session and collect the LLM response,
/// surfacing tool activity to the text channel along the way. Runs without
/// holding `voice_mgr.sessions.lock()` so skill tools that need the same lock
/// (e.g. `leave_voice`) can run concurrently.
async fn process_llm_turn(
    daemon_session: &mut simply_daemon::DaemonSession,
    user_text: &str,
    text_channel: &ChannelId,
    http: &Arc<serenity::http::Http>,
) -> Option<String> {
    let send_result = daemon_session.send(simply_daemon_api::UserMessage {
        content: vec![simply_daemon_api::InputContent::Text { text: user_text.to_string() }],
    }).await;
    if let Err(e) = send_result {
        tracing::error!(error = %e, "failed to send to session");
        return None;
    }

    let mut response_text = String::new();
    loop {
        match daemon_session.recv().await {
            Ok(simply_daemon_api::DaemonEvent::TextDelta(delta)) => {
                response_text.push_str(&delta);
            }
            Ok(simply_daemon_api::DaemonEvent::ToolCall { name, arguments, .. }) => {
                tracing::info!(tool = %name, "voice tool call");
                let preview = truncate_preview(&serde_json::to_string(&arguments).unwrap_or_default(), 200);
                let _ = text_channel.say(http, format!("🔧 `{name}` {preview}")).await;
            }
            Ok(simply_daemon_api::DaemonEvent::ToolResult { result, .. }) => {
                post_tool_result(&result, text_channel, http).await;
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

                            // Take the DaemonSession out of the map so tool calls (e.g.
                            // leave_voice, which takes sessions.lock()) can run concurrently
                            // with the send/recv loop and don't deadlock on our lock.
                            let daemon_session = {
                                let mut sessions = voice_mgr.sessions.lock().await;
                                sessions.get_mut(&guild_id).and_then(|s| s.daemon_session.take())
                            };

                            let (response, daemon_session) = if let Some(mut ds) = daemon_session {
                                let response = process_llm_turn(&mut ds, &text, &text_channel, &http).await;
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
    });
}
