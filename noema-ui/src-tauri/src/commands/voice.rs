//! Voice-related Tauri commands

use noema_audio::{
    create_browser_backend, VoiceAgent, VoiceCoordinator,
};

#[cfg(feature = "native-audio")]
use noema_audio::StreamingAudioCapture;

use tauri::{AppHandle, Emitter, Manager, State};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

use crate::logging::log_message;
use crate::state::AppState;

/// Check if voice is available (Whisper model exists)
#[tauri::command]
pub async fn is_voice_available(app: AppHandle) -> Result<bool, String> {
    let model_path = get_whisper_model_path(&app).ok_or("Could not determine model path")?;
    Ok(model_path.exists())
}

/// Get the Whisper model path using AppHandle for proper mobile resolution
fn get_whisper_model_path(app: &AppHandle) -> Option<PathBuf> {
    // On all platforms, prefer app_data_dir
    // This works for ~/.local/share/noema on Linux/macOS
    // and internal storage on Android/iOS
    app.path().app_data_dir()
        .ok()
        .map(|dir| dir.join("models").join("ggml-base.en.bin"))
}

/// Download the Whisper model
#[tauri::command]
pub async fn download_voice_model(app: AppHandle) -> Result<(), String> {
    let model_path = get_whisper_model_path(&app)
        .ok_or("Could not determine model path")?;

    if model_path.exists() {
        return Ok(());
    }

    if let Some(parent) = model_path.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| format!("Failed to create model directory: {}", e))?;
    }

    let url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";
    log_message(&format!("Downloading model from {}", url));
    app.emit("download_progress", "starting").ok();

    let client = reqwest::Client::new();
    let response = client.get(url).send().await
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

/// Spawn the event polling loop for the voice coordinator
fn spawn_voice_loop(app: AppHandle) {
    tokio::spawn(async move {
        let state = app.state::<AppState>();
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
                    // Voice was disabled or session ended
                    break;
                }
            };

            // Emit status updates
            if is_listening {
                app.emit("voice_status", "listening").ok();
            } else if is_transcribing {
                app.emit("voice_status", "transcribing").ok();
            } else {
                app.emit("voice_status", "enabled").ok();
            }

            // Send transcribed messages as chat messages
            for message in messages {
                app.emit("voice_transcription", &message).ok();
            }

            // Report errors
            for error in errors {
                app.emit("voice_error", &error).ok();
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        // When loop exits, ensure status is disabled
        app.emit("voice_status", "disabled").ok();
    });
}

/// Toggle voice input on/off (Native)
#[tauri::command]
pub async fn toggle_voice(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let mut coordinator_guard = state.voice_coordinator.lock().await;

    if coordinator_guard.is_some() {
        // Disable voice - just set to None
        *coordinator_guard = None;
        // Loop will exit automatically
        Ok(false)
    } else {
        // Enable voice
        #[cfg(feature = "native-audio")]
        {
            let model_path = get_whisper_model_path(&app).ok_or(
                "Whisper model not found. Please download it first.",
            )?;

            if !model_path.exists() {
                return Err("Model file not found. Please download it.".to_string());
            }

            let streamer = StreamingAudioCapture::new()
                .map_err(|e| format!("Failed to initialize audio capture: {}", e))?;

            let agent = VoiceAgent::new(Box::new(streamer), &model_path)
                .map_err(|e| format!("Failed to start voice agent: {}", e))?;

            let coordinator = VoiceCoordinator::new(agent);
            *coordinator_guard = Some(coordinator);
            drop(coordinator_guard); // Release lock before spawning

            spawn_voice_loop(app.clone());

            app.emit("voice_status", "enabled").ok();
            Ok(true)
        }
        #[cfg(not(feature = "native-audio"))]
        {
            // Suppress unused variable warning
            let _ = app;
            Err("Native voice not supported in this build".to_string())
        }
    }
}

/// Get current voice status
#[tauri::command]
pub async fn get_voice_status(state: State<'_, AppState>) -> Result<String, String> {
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
    // Ensure no existing session
    {
        let coordinator = state.voice_coordinator.lock().await;
        if coordinator.is_some() {
            // Already active (either native or browser)
            return Ok(()); 
        }
        // Also clear controller
        *state.browser_audio_controller.lock().await = None;
    }

    let model_path = get_whisper_model_path(&app).ok_or(
        "Whisper model not found. Please download it first.",
    )?;

    if !model_path.exists() {
        return Err("Model file not found. Please download it.".to_string());
    }

    // Create browser backend (controller + streamer)
    // Assuming browser sends 16kHz or we handle resampling. 
    // For now, let's assume 16000, but we might need to parameterize this if browser sends 44100.
    // Ideally, we'd pass the sample rate from the frontend.
    let (controller, streamer) = create_browser_backend(16000);

    let agent = VoiceAgent::new(Box::new(streamer), &model_path)
        .map_err(|e| format!("Failed to start voice session: {}", e))?;

    let coordinator = VoiceCoordinator::new(agent);

    // Store state
    *state.browser_audio_controller.lock().await = Some(controller);
    *state.voice_coordinator.lock().await = Some(coordinator);

    // Start loop
    spawn_voice_loop(app.clone());

    app.emit("voice_status", "listening").ok();
    log_message("Browser voice session started");

    Ok(())
}

/// Process audio samples from browser WebAudio API
#[tauri::command]
pub async fn process_audio_chunk(
    _app: AppHandle,
    state: State<'_, AppState>,
    samples: Vec<f32>,
) -> Result<(), String> {
    let controller_guard = state.browser_audio_controller.lock().await;
    let controller = controller_guard.as_ref().ok_or("No active voice session")?;

    // Push to backend
    controller.process_samples(&samples);

    // We don't need to check "is_speech_active" here because the loop handles status updates
    Ok(())
}

/// Stop the browser voice session and get final transcription
#[tauri::command]
pub async fn stop_voice_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    // 1. Finish the controller (flush VAD)
    {
        let controller_guard = state.browser_audio_controller.lock().await;
        if let Some(controller) = controller_guard.as_ref() {
            controller.finish();
        }
    }

    // 2. Wait a bit for final transcription? 
    // The agent runs in background. If we drop coordinator immediately, we might lose pending transcription.
    // But we can't easily "wait" for the agent to finish transcribing unless we change VoiceAgent API.
    // For now, we just stop. 
    
    // Drop coordinator to stop the loop and agent
    {
        let mut coordinator = state.voice_coordinator.lock().await;
        *coordinator = None;
    }
    {
        let mut controller = state.browser_audio_controller.lock().await;
        *controller = None;
    }

    app.emit("voice_status", "disabled").ok();
    log_message("Stopping browser voice session");
    
    Ok(None) // We don't return final text synchronously anymore, it comes via events
}