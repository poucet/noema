//! Browser-based voice input support
//!
//! This module handles audio samples streamed from browser WebAudio API
//! and processes them through VAD and Whisper transcription.

use crate::audio::{SpeechEvent, VoiceActivityDetector};
use crate::transcription::Transcriber;
use anyhow::Result;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Session for processing browser audio input
pub struct BrowserVoiceSession {
    transcriber: Arc<Transcriber>,
    vad: Mutex<VoiceActivityDetector>,
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

        // Process through VAD
        if let Some(event) = vad.process_samples(samples) {
            match event {
                SpeechEvent::SpeechStart { .. } => {
                    *is_active = true;
                    buffer.clear();
                    buffer.extend_from_slice(samples);
                }
                SpeechEvent::SpeechChunk(segment) => {
                    if *is_active {
                        buffer.extend_from_slice(&segment.audio_data);
                    }
                }
                SpeechEvent::SpeechEnd(segment) => {
                    *is_active = false;
                    buffer.extend_from_slice(&segment.audio_data);

                    // Transcribe the complete utterance
                    let samples_to_transcribe = buffer.clone();
                    buffer.clear();

                    if samples_to_transcribe.len() > 1600 {
                        // At least 100ms of audio
                        match self.transcriber.transcribe(&samples_to_transcribe) {
                            Ok(text) if !text.is_empty() => return Some(text),
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("Transcription error: {}", e);
                            }
                        }
                    }
                }
            }
        } else if *is_active {
            // Accumulate samples during active speech
            buffer.extend_from_slice(samples);
        }

        None
    }

    /// Force transcription of any buffered audio (called when recording stops)
    pub fn finish(&self) -> Option<String> {
        let mut buffer = self.audio_buffer.lock().unwrap();
        let mut is_active = self.is_speech_active.lock().unwrap();

        *is_active = false;

        if buffer.len() > 1600 {
            let samples = buffer.clone();
            buffer.clear();

            match self.transcriber.transcribe(&samples) {
                Ok(text) if !text.is_empty() => return Some(text),
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Transcription error: {}", e);
                }
            }
        }

        buffer.clear();
        None
    }

    /// Check if speech is currently detected
    pub fn is_speech_active(&self) -> bool {
        *self.is_speech_active.lock().unwrap()
    }
}
