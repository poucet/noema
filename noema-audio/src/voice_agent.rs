//! Voice-enabled agent wrapper
//!
//! Wraps any Agent to add voice input capabilities via microphone
//! and speech-to-text transcription.

use crate::audio::{SpeechEvent, StreamingAudioCapture};
use crate::transcription::Transcriber;
use anyhow::Result;
use std::path::Path;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Events from the voice agent
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// Speech detection started (user is speaking)
    ListeningStarted,
    /// Speech ended and transcription is available
    Transcription(String),
    /// Agent response text
    Response(String),
    /// An error occurred
    Error(String),
}

/// Voice-enabled agent that wraps any underlying agent
///
/// Provides voice input via microphone, transcribes speech to text,
/// and forwards to the wrapped agent for processing.
pub struct VoiceAgent {
    #[allow(dead_code)]
    audio_capture: Option<StreamingAudioCapture>,
    event_rx: Option<mpsc::UnboundedReceiver<VoiceEvent>>,
}

impl VoiceAgent {
    /// Create a new voice agent and start the voice session
    ///
    /// # Arguments
    /// * `model_path` - Path to the Whisper GGML model file
    pub fn new(model_path: impl AsRef<Path>) -> Result<Self> {
        // Validate the model exists by creating a transcriber
        let _ = Transcriber::new(model_path.as_ref())?;

        let mut capture = StreamingAudioCapture::new()?;
        let speech_rx = capture.start_streaming()?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let model_path = model_path.as_ref().to_string_lossy().to_string();

        std::thread::spawn(move || {
            info!("Voice transcription thread started");

            // Create transcriber in background thread
            let transcriber = match Transcriber::new(&model_path) {
                Ok(t) => {
                    info!("Transcriber initialized successfully");
                    t
                }
                Err(e) => {
                    error!("Failed to initialize transcriber: {}", e);
                    let _ = event_tx.send(VoiceEvent::Error(format!(
                        "Failed to initialize transcriber: {}",
                        e
                    )));
                    return;
                }
            };

            info!("Waiting for speech events...");
            for event in speech_rx {
                match event {
                    SpeechEvent::SpeechStart { .. } => {
                        debug!("Speech started");
                        if event_tx.send(VoiceEvent::ListeningStarted).is_err() {
                            warn!("Failed to send ListeningStarted event - receiver dropped");
                            break;
                        }
                    }
                    SpeechEvent::SpeechChunk(_) => {
                        // Intermediate chunks - could be used for streaming transcription
                    }
                    SpeechEvent::SpeechEnd(segment) => {
                        let duration_ms = segment.duration_ms();
                        debug!("Speech ended, duration: {:.0}ms, samples: {}", duration_ms, segment.audio_data.len());

                        match transcriber.transcribe(&segment.audio_data) {
                            Ok(text) if !text.trim().is_empty() => {
                                info!("Transcription: {:?}", text);
                                if event_tx.send(VoiceEvent::Transcription(text)).is_err() {
                                    warn!("Failed to send Transcription event - receiver dropped");
                                    break;
                                }
                            }
                            Ok(_) => {
                                debug!("Empty transcription, ignoring");
                            }
                            Err(e) => {
                                error!("Transcription failed: {}", e);
                                if event_tx.send(VoiceEvent::Error(format!(
                                    "Transcription failed: {}",
                                    e
                                ))).is_err() {
                                    warn!("Failed to send Error event - receiver dropped");
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            info!("Voice transcription thread exiting - speech_rx channel closed");
        });

        Ok(Self {
            audio_capture: Some(capture),
            event_rx: Some(event_rx),
        })
    }

    /// Try to receive a voice event without blocking
    pub fn try_recv(&mut self) -> Option<VoiceEvent> {
        match self.event_rx.as_mut()?.try_recv() {
            Ok(event) => Some(event),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => None,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                warn!("VoiceAgent event channel disconnected");
                None
            }
        }
    }
}
