use anyhow::Result;
use std::sync::mpsc::Receiver;

use crate::types::SpeechEvent;

/// Trait for audio capture streaming with VAD
pub trait AudioStreamer: Send + Sync {
    /// Start capturing audio and return a channel for speech events
    fn start_streaming(&mut self) -> Result<Receiver<SpeechEvent>>;
}

/// Trait for audio playback
pub trait AudioPlayer: Send + Sync {
    /// Play audio samples (16kHz mono f32)
    fn play(&self, samples: &[f32]) -> Result<()>;
}
