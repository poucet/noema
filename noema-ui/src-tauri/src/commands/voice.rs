//! Voice-related Tauri commands

use noema_audio::{BrowserVoiceSession, VoiceAgent, VoiceCoordinator};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::logging::log_message;
use crate::state::AppState;

/// Check if voice is available (Whisper model exists)
#[tauri::command]
pub async fn is_voice_available() -> Result<bool, String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let data_dir = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let model_path = data_dir.join("noema").join("models").join("ggml-base.en.bin");
        Ok(model_path.exists())
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Ok(false) // Voice not supported on mobile yet
    }
}

/// Get the Whisper model path
fn get_whisper_model_path() -> Option<std::path::PathBuf> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let data_dir = dirs::data_dir()?;
        let model_path = data_dir.join("noema").join("models").join("ggml-base.en.bin");
        if model_path.exists() {
            Some(model_path)
        } else {
            None
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        None
    }
}

/// Toggle voice input on/off
#[tauri::command]
pub async fn toggle_voice(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let mut coordinator_guard = state.voice_coordinator.lock().await;

    if coordinator_guard.is_some() {
        // Disable voice - just set to None
        *coordinator_guard = None;
        app.emit("voice_status", "disabled").ok();
        Ok(false)
    } else {
        // Enable voice
        let model_path = get_whisper_model_path().ok_or(
            "Whisper model not found. Please download ggml-base.en.bin to ~/.local/share/noema/models/",
        )?;

        let agent = VoiceAgent::new(&model_path)
            .map_err(|e| format!("Failed to start voice agent: {}", e))?;

        let coordinator = VoiceCoordinator::new(agent);
        *coordinator_guard = Some(coordinator);
        drop(coordinator_guard); // Release lock before spawning

        // Start polling for voice events
        let app_handle = app.clone();
        tokio::spawn(async move {
            let state = app_handle.state::<AppState>();
            loop {
                // Check if we're currently processing a message - if so, buffer voice input
                let is_processing = *state.is_processing.lock().await;

                let (messages, errors, is_listening, is_transcribing) = {
                    let mut coordinator_guard = state.voice_coordinator.lock().await;
                    if let Some(coordinator) = coordinator_guard.as_mut() {
                        let is_listening = coordinator.is_listening();
                        let is_transcribing = coordinator.is_transcribing();
                        // Buffer messages while processing, release when not processing
                        let (msgs, errs) = coordinator.process(is_processing);
                        (msgs, errs, is_listening, is_transcribing)
                    } else {
                        // Voice was disabled
                        break;
                    }
                };

                // Emit status updates
                if is_listening {
                    app_handle.emit("voice_status", "listening").ok();
                } else if is_transcribing {
                    app_handle.emit("voice_status", "transcribing").ok();
                } else {
                    app_handle.emit("voice_status", "enabled").ok();
                }

                // Send transcribed messages as chat messages
                for message in messages {
                    app_handle.emit("voice_transcription", &message).ok();
                }

                // Report errors
                for error in errors {
                    app_handle.emit("voice_error", &error).ok();
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        });

        app.emit("voice_status", "enabled").ok();
        Ok(true)
    }
}

/// Get current voice status
#[tauri::command]
pub async fn get_voice_status(state: State<'_, AppState>) -> Result<String, String> {
    // Check browser voice session first
    let browser_session = state.browser_voice_session.lock().await;
    if browser_session.is_some() {
        return Ok("listening".to_string());
    }
    drop(browser_session);

    let coordinator_guard = state.voice_coordinator.lock().await;
    if let Some(coordinator) = coordinator_guard.as_ref() {
        if coordinator.is_listening() {
            Ok("listening".to_string())
        } else if coordinator.is_transcribing() {
            Ok("transcribing".to_string())
        } else {
            Ok("enabled".to_string())
        }
    } else {
        Ok("disabled".to_string())
    }
}

// ============================================================================
// Browser Voice Commands (WebAudio-based)
// ============================================================================

/// Start a browser voice session
#[tauri::command]
pub async fn start_voice_session(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let model_path = get_whisper_model_path().ok_or(
        "Whisper model not found. Please download ggml-base.en.bin to ~/.local/share/noema/models/",
    )?;

    let session = BrowserVoiceSession::new(&model_path)
        .map_err(|e| format!("Failed to start voice session: {}", e))?;

    *state.browser_voice_session.lock().await = Some(session);
    app.emit("voice_status", "listening").ok();
    log_message("Browser voice session started");

    Ok(())
}

/// Process audio samples from browser WebAudio API
#[tauri::command]
pub async fn process_audio_chunk(
    app: AppHandle,
    state: State<'_, AppState>,
    samples: Vec<f32>,
) -> Result<(), String> {
    let session_guard = state.browser_voice_session.lock().await;
    let session = session_guard.as_ref().ok_or("No active voice session")?;

    // Process samples through VAD and transcription
    if let Some(transcription) = session.process_samples(&samples) {
        log_message(&format!("Transcription: {}", transcription));
        app.emit("voice_transcription", &transcription).ok();
    }

    // Update status based on speech detection
    if session.is_speech_active() {
        app.emit("voice_status", "listening").ok();
    }

    Ok(())
}

/// Stop the browser voice session and get final transcription
#[tauri::command]
pub async fn stop_voice_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let mut session_guard = state.browser_voice_session.lock().await;

    if let Some(session) = session_guard.take() {
        app.emit("voice_status", "transcribing").ok();
        log_message("Stopping browser voice session");

        // Get any remaining transcription
        let final_text = session.finish();

        if let Some(ref text) = final_text {
            log_message(&format!("Final transcription: {}", text));
            app.emit("voice_transcription", text).ok();
        }

        app.emit("voice_status", "disabled").ok();
        Ok(final_text)
    } else {
        Ok(None)
    }
}
