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

use crate::logging::log_message;
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

    log_message(&format!("Downloading model from {}", url));
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

    log_message("Model download complete");
    app.emit("download_progress", "complete").ok();
    Ok(())
}

/// Spawn a loop that receives VoiceEvents from the daemon and emits Tauri events.
fn spawn_voice_event_loop(
    app: AppHandle,
    mut event_rx: mpsc::Receiver<VoiceEvent>,
) {
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                VoiceEvent::Listening => {
                    app.emit("voice_status", "listening").ok();
                }
                VoiceEvent::Transcribing => {
                    app.emit("voice_status", "transcribing").ok();
                }
                VoiceEvent::UserTranscript(text) => {
                    app.emit("voice_status", "enabled").ok();
                    app.emit("voice_transcription", &text).ok();
                }
                VoiceEvent::ModelTranscript(_) => {
                    // Not used in pipeline mode yet
                }
                VoiceEvent::Audio(_) => {
                    // TTS playback — not implemented yet
                }
                VoiceEvent::TurnEnd => {
                    app.emit("voice_status", "enabled").ok();
                }
                VoiceEvent::Error(e) => {
                    app.emit("voice_error", &e).ok();
                    app.emit("voice_status", "enabled").ok();
                }
            }
        }
        // Channel closed — session ended
        app.emit("voice_status", "disabled").ok();
    });
}

/// List available voice providers.
#[tauri::command]
pub async fn list_voice_providers(state: State<'_, Arc<AppState>>) -> Result<Vec<simply_daemon::api::VoiceProviderInfo>, String> {
    let daemon = state.get_daemon()?;
    daemon.voice().list_voice_providers().await
        .map_err(|e| format!("Failed to list voice providers: {e}"))
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

    let daemon = state.get_daemon()?;

    let handle = daemon.voice().voice_connect(&provider_id).await
        .map_err(|e| format!("Failed to connect voice: {e}"))?;

    let (input_tx, event_rx) = handle.into_parts();
    *state.voice_audio_tx.lock().await = Some(input_tx);

    spawn_voice_event_loop(app.clone(), event_rx);

    app.emit("voice_status", "enabled").ok();
    log_message("Voice session started (daemon pipeline)");

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
    log_message("Voice session stopped");

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
        start_voice_session(app, state).await?;
        Ok(true)
    }
}

/// Get current voice status
#[tauri::command]
pub async fn get_voice_status(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let has_session = state.voice_audio_tx.lock().await.is_some();
    Ok(if has_session { "enabled" } else { "disabled" }.to_string())
}
