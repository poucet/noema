//! Voice-related Tauri commands
//!
//! Audio capture (CPAL/browser) happens in Noema. Audio is streamed to the
//! daemon's VoiceApi which runs VAD → STT → events back to the client.

use simply_voice::{AudioChunk, VoiceEvent, VoiceInput};

use tauri::{AppHandle, Emitter, Manager, State};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing;

use crate::state::AppState;

/// Check if voice is available (Whisper model exists)
#[tauri::command]
pub async fn is_voice_available(app: AppHandle) -> Result<bool, String> {
    let model_path = get_whisper_model_path(&app).ok_or("Could not determine model path")?;
    Ok(model_path.exists())
}

/// Get the Whisper model path using AppHandle for proper mobile resolution
fn get_whisper_model_path(_app: &AppHandle) -> Option<PathBuf> {
    use config::PathManager;
    PathManager::whisper_model_path()
}

/// Download the Whisper model
#[tauri::command]
pub async fn download_voice_model(app: AppHandle, url: String) -> Result<(), String> {
    let model_path = get_whisper_model_path(&app)
        .ok_or("Could not determine model path")?;

    if model_path.exists() {
        return Ok(());
    }

    if let Some(parent) = model_path.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| format!("Failed to create model directory: {}", e))?;
    }

    tracing::info!(url = %url, "downloading voice model");
    app.emit("download_progress", "starting").ok();

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await
        .map_err(|e| format!("Failed to fetch model: {}", e))?;

    let total_size = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&model_path).await
        .map_err(|e| format!("Failed to create model file: {}", e))?;

    let mut downloaded: u64 = 0;

    use futures::StreamExt;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk).await.map_err(|e| format!("Write error: {}", e))?;

        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let progress = (downloaded as f64 / total_size as f64 * 100.0) as u8;
            app.emit("download_progress", progress).ok();
        }
    }

    tracing::info!("Model download complete");
    app.emit("download_progress", "complete").ok();
    Ok(())
}

/// Spawn a loop that receives VoiceEvents from the daemon and emits Tauri events.
fn spawn_voice_event_loop(
    app: AppHandle,
    mut event_rx: mpsc::Receiver<VoiceEvent>,
) {
    tokio::spawn(async move {
        tracing::debug!("voice event loop started");
        while let Some(event) = event_rx.recv().await {
            match event {
                VoiceEvent::Listening => {
                    tracing::debug!("voice: listening");
                    app.emit("voice_status", "listening").ok();
                }
                VoiceEvent::Transcribing => {
                    tracing::debug!("voice: transcribing");
                    app.emit("voice_status", "transcribing").ok();
                }
                VoiceEvent::UserTranscript(ref text) => {
                    tracing::info!(text = %text, "voice: user transcript");
                    app.emit("voice_status", "enabled").ok();
                    app.emit("voice_transcription", text).ok();
                }
                VoiceEvent::ModelTranscript(ref text) => {
                    tracing::info!(text = %text, "voice: model transcript");
                }
                VoiceEvent::Audio(ref chunk) => {
                    tracing::debug!(bytes = chunk.data.len(), "voice: audio out");
                }
                VoiceEvent::TurnEnd => {
                    tracing::debug!("voice: turn end");
                    app.emit("voice_status", "enabled").ok();
                }
                VoiceEvent::Error(ref e) => {
                    tracing::error!(error = %e, "voice: error");
                    app.emit("voice_error", e).ok();
                    app.emit("voice_status", "enabled").ok();
                }
            }
        }
        tracing::debug!("voice event loop ended");
        app.emit("voice_status", "disabled").ok();
    });
}

/// List available voice providers.
#[tauri::command]
pub async fn list_voice_providers(state: State<'_, Arc<AppState>>) -> Result<Vec<simply_daemon::api::VoiceProviderInfo>, String> {
    let daemon = state.get_daemon()?;
    let providers = daemon.voice().list_voice_providers().await
        .map_err(|e| format!("Failed to list voice providers: {e}"))?;
    tracing::info!(count = providers.len(), providers = ?providers.iter().map(|p| &p.id).collect::<Vec<_>>(), "voice providers loaded");
    Ok(providers)
}

/// Start a browser voice session — connects to the daemon's VoiceApi.
#[tauri::command]
pub async fn start_voice_session(app: AppHandle, state: State<'_, Arc<AppState>>, provider_id: String) -> Result<(), String> {
    {
        let existing = state.voice_audio_tx.lock().await;
        if existing.is_some() {
            return Ok(()); // Already active
        }
    }

    tracing::info!(provider_id = %provider_id, "starting voice session");

    let daemon = state.get_daemon()?;

    let handle = daemon.voice().voice_connect(&provider_id).await
        .map_err(|e| format!("Failed to connect voice: {e}"))?;

    tracing::info!("voice stream connected");

    let (input_tx, event_rx) = handle.into_parts();
    *state.voice_audio_tx.lock().await = Some(input_tx);

    spawn_voice_event_loop(app.clone(), event_rx);

    app.emit("voice_status", "enabled").ok();
    tracing::info!("voice session started");

    Ok(())
}

/// Process audio samples from browser WebAudio API — forward to daemon.
#[tauri::command]
pub async fn process_audio_chunk(
    _app: AppHandle,
    state: State<'_, Arc<AppState>>,
    samples: Vec<f32>,
) -> Result<(), String> {
    let tx_guard = state.voice_audio_tx.lock().await;
    let tx = tx_guard.as_ref().ok_or("No active voice session")?;

    // Convert f32 samples to PCM16 LE bytes
    let bytes: Vec<u8> = samples.iter()
        .flat_map(|&s| {
            let clamped = s.clamp(-1.0, 1.0);
            let i = (clamped * 32767.0) as i16;
            i.to_le_bytes()
        })
        .collect();

    let input = VoiceInput::Audio(AudioChunk::new(bytes));
    tx.send(input).await.map_err(|_| "Voice session closed".to_string())?;

    Ok(())
}

/// Stop the browser voice session.
#[tauri::command]
pub async fn stop_voice_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    // Drop the sender — this closes the channel, ending the daemon pipeline
    *state.voice_audio_tx.lock().await = None;

    app.emit("voice_status", "disabled").ok();
    tracing::info!("Voice session stopped");

    Ok(None)
}

/// Toggle voice input on/off (Native — uses CPAL directly)
#[tauri::command]
pub async fn toggle_voice(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    let has_session = state.voice_audio_tx.lock().await.is_some();

    if has_session {
        stop_voice_session(app, state).await?;
        Ok(false)
    } else {
        // Use first available provider as default
        let daemon = state.get_daemon()?;
        let providers = daemon.voice().list_voice_providers().await
            .map_err(|e| format!("{e}"))?;
        let provider_id = providers.first()
            .map(|p| p.id.clone())
            .ok_or("No voice providers available")?;
        start_voice_session(app, state, provider_id).await?;
        Ok(true)
    }
}

/// Get current voice status
#[tauri::command]
pub async fn get_voice_status(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let has_session = state.voice_audio_tx.lock().await.is_some();
    Ok(if has_session { "enabled" } else { "disabled" }.to_string())
}

/// Synthesize text to speech. Returns f32 samples ready for Web Audio API playback.
#[tauri::command]
pub async fn synthesize_speech(
    state: State<'_, Arc<AppState>>,
    text: String,
    provider_id: String,
    voice: Option<String>,
) -> Result<SynthesizeResult, String> {
    let voice = voice.as_deref().unwrap_or("");
    tracing::info!(provider_id = %provider_id, voice = %voice, text_len = text.len(), "TTS: synthesizing");

    let daemon = state.get_daemon()?;
    let chunk = daemon.voice().synthesize(&text, &provider_id, voice).await
        .map_err(|e| {
            tracing::error!(error = %e, "TTS: synthesis failed");
            format!("TTS failed: {e}")
        })?;

    tracing::info!(audio_bytes = chunk.data.len(), sample_rate = chunk.sample_rate, "TTS: got audio");

    // Debug: write raw audio to a file so we can verify it
    if let Ok(mut f) = std::fs::File::create("/tmp/tts_debug.raw") {
        use std::io::Write;
        let _ = f.write_all(&chunk.data);
        tracing::info!("TTS: debug audio written to /tmp/tts_debug.raw (play with: ffplay -f s16le -ar {} -ac 1 /tmp/tts_debug.raw)", chunk.sample_rate);
    }

    // Convert PCM16 LE to f32 samples for Web Audio API
    let samples: Vec<f32> = chunk.data.chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
        .collect();

    Ok(SynthesizeResult {
        samples,
        sample_rate: chunk.sample_rate,
    })
}

#[derive(serde::Serialize)]
pub struct SynthesizeResult {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}
