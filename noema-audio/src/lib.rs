//! Audio capabilities for noema
//!
//! This crate provides:
//! - Audio capture and playback via `cpal` (feature: `backend-cpal`)
//! - Voice activity detection (VAD)
//! - Speech-to-text transcription via Whisper
//! - Voice-enabled agent wrapper
//! - Browser audio streaming support (feature: `browser`)

pub mod traits;
pub mod types;
pub mod utils;
pub mod vad;

#[cfg(feature = "backend-cpal")]
pub mod cpal_backend;

#[cfg(not(feature = "backend-cpal"))]
pub mod dummy_backend;

#[cfg(feature = "browser")]
pub mod browser_backend;

pub mod coordinator;
pub mod transcription;
pub mod voice_agent;

// Re-export types
pub use types::{AudioSegment, SpeechEvent};
pub use traits::{AudioPlayer, AudioStreamer};

// Default backend exports
#[cfg(feature = "backend-cpal")]
pub use cpal_backend::{CpalAudioCapture as AudioCapture, CpalAudioPlayer as AudioPlayback, CpalAudioStreamer as StreamingAudioCapture};

#[cfg(not(feature = "backend-cpal"))]
pub use dummy_backend::{DummyAudioCapture as AudioCapture, DummyAudioPlayer as AudioPlayback, DummyAudioStreamer as StreamingAudioCapture};

#[cfg(feature = "browser")]
pub use browser_backend::{create_browser_backend, BrowserAudioController, BrowserAudioStreamer};

pub use coordinator::VoiceCoordinator;
pub use transcription::Transcriber;
pub use voice_agent::{VoiceAgent, VoiceEvent};