use std::time::Instant;

/// Audio segment with timestamp and normalized 16kHz data
#[derive(Debug, Clone)]
pub struct AudioSegment {
    pub timestamp: Instant,
    pub audio_data: Vec<f32>,
}

impl AudioSegment {
    pub fn new(timestamp: Instant, audio_data: Vec<f32>) -> Self {
        Self {
            timestamp,
            audio_data,
        }
    }

    pub fn duration_ms(&self) -> f32 {
        (self.audio_data.len() as f32 / 16000.0) * 1000.0
    }
}

/// Events emitted during speech detection
#[derive(Debug, Clone)]
pub enum SpeechEvent {
    /// Speech has started
    SpeechStart { timestamp: Instant },
    /// Speech has ended with complete audio segment
    SpeechEnd(AudioSegment),
    /// Intermediate speech chunk during active speech
    SpeechChunk(AudioSegment),
}
