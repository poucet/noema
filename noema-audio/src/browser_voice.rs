//! Browser-based voice input support
//!
//! This module handles audio samples streamed from browser WebAudio API
//! and processes them through VAD and Whisper transcription.
//!
//! Note: The VAD (VoiceActivityDetector) internally accumulates all audio
//! during speech and returns the complete audio in SpeechEnd. We maintain
//! a separate buffer only for the `finish()` method (when user stops recording
//! before VAD detects silence).

use crate::audio::{SpeechEvent, VoiceActivityDetector};
use crate::transcription::Transcriber;
use anyhow::Result;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Log a message to ~/Library/Logs/Noema/noema.log
fn log_message(msg: &str) {
    if let Some(home) = dirs::home_dir() {
        let log_dir = home.join("Library/Logs/Noema");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("noema.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] [BrowserVoice] {}", timestamp, msg);
        }
    }
}

/// Session for processing browser audio input
pub struct BrowserVoiceSession {
    transcriber: Arc<Transcriber>,
    vad: Mutex<VoiceActivityDetector>,
    /// Buffer for finish() - only used when user stops recording manually
    audio_buffer: Mutex<Vec<f32>>,
    is_speech_active: Mutex<bool>,
}

impl BrowserVoiceSession {
    /// Create a new browser voice session
    pub fn new(model_path: impl AsRef<Path>) -> Result<Self> {
        let transcriber = Arc::new(Transcriber::new(model_path)?);
        let vad = VoiceActivityDetector::new(16000); // Browser sends 16kHz audio

        Ok(Self {
            transcriber,
            vad: Mutex::new(vad),
            audio_buffer: Mutex::new(Vec::new()),
            is_speech_active: Mutex::new(false),
        })
    }

    /// Process incoming audio samples (16kHz mono f32)
    /// Returns transcription if speech segment completed
    pub fn process_samples(&self, samples: &[f32]) -> Option<String> {
        let mut vad = self.vad.lock().unwrap();
        let mut buffer = self.audio_buffer.lock().unwrap();
        let mut is_active = self.is_speech_active.lock().unwrap();

        // Always accumulate samples for finish() fallback
        buffer.extend_from_slice(samples);

        // Process through VAD - it internally accumulates audio and returns
        // the complete speech in SpeechEnd
        if let Some(event) = vad.process_samples(samples) {
            match event {
                SpeechEvent::SpeechStart { .. } => {
                    log_message("SpeechStart detected");
                    *is_active = true;
                    // Clear our buffer since VAD is tracking now
                    buffer.clear();
                    buffer.extend_from_slice(samples);
                }
                SpeechEvent::SpeechChunk(_) => {
                    // VAD is accumulating internally, we just track state
                }
                SpeechEvent::SpeechEnd(segment) => {
                    // VAD returns ALL accumulated audio in segment.audio_data
                    let samples_to_transcribe = &segment.audio_data;
                    log_message(&format!(
                        "SpeechEnd detected, transcribing {} samples",
                        samples_to_transcribe.len()
                    ));
                    *is_active = false;
                    buffer.clear();

                    if samples_to_transcribe.len() > 1600 {
                        // At least 100ms of audio
                        match self.transcriber.transcribe(samples_to_transcribe) {
                            Ok(text) if !text.is_empty() => {
                                log_message(&format!("Transcription: {}", text));
                                return Some(text);
                            }
                            Ok(_) => {
                                log_message("Transcription returned empty");
                            }
                            Err(e) => {
                                log_message(&format!("Transcription error: {}", e));
                            }
                        }
                    } else {
                        log_message(&format!(
                            "Audio too short: {} samples (need >1600)",
                            samples_to_transcribe.len()
                        ));
                    }
                }
            }
        }

        None
    }

    /// Force transcription of any buffered audio (called when recording stops)
    pub fn finish(&self) -> Option<String> {
        let mut buffer = self.audio_buffer.lock().unwrap();
        let mut is_active = self.is_speech_active.lock().unwrap();

        *is_active = false;

        if buffer.len() > 1600 {
            log_message(&format!("finish() transcribing {} buffered samples", buffer.len()));
            let samples = buffer.clone();
            buffer.clear();

            match self.transcriber.transcribe(&samples) {
                Ok(text) if !text.is_empty() => {
                    log_message(&format!("finish() transcription: {}", text));
                    return Some(text);
                }
                Ok(_) => {
                    log_message("finish() transcription returned empty");
                }
                Err(e) => {
                    log_message(&format!("finish() transcription error: {}", e));
                }
            }
        } else {
            log_message(&format!("finish() buffer too short: {} samples", buffer.len()));
        }

        buffer.clear();
        None
    }

    /// Check if speech is currently detected
    pub fn is_speech_active(&self) -> bool {
        *self.is_speech_active.lock().unwrap()
    }
}
