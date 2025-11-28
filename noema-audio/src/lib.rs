//! Audio capabilities for noema
//!
//! This crate provides:
//! - Audio capture and playback via `cpal`
//! - Voice activity detection (VAD)
//! - Speech-to-text transcription via Whisper
//! - Voice-enabled agent wrapper
//!
//! # Example
//!
//! ```ignore
//! use noema_audio::{VoiceAgent, VoiceEvent};
//!
//! let mut voice_agent = VoiceAgent::new("models/ggml-base.en.bin")?;
//! let mut events = voice_agent.start_voice_session()?;
//!
//! while let Some(event) = events.recv().await {
//!     match event {
//!         VoiceEvent::Transcription(text) => println!("You said: {}", text),
//!         VoiceEvent::ListeningStarted => println!("Listening..."),
//!         _ => {}
//!     }
//! }
//! ```

pub mod audio;
pub mod coordinator;
pub mod transcription;
pub mod voice_agent;

pub use audio::{AudioCapture, AudioPlayback, AudioSegment, SpeechEvent, StreamingAudioCapture};
pub use coordinator::VoiceCoordinator;
pub use transcription::Transcriber;
pub use voice_agent::{VoiceAgent, VoiceEvent};