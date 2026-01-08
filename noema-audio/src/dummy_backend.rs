use anyhow::{anyhow, Result};
use std::sync::mpsc::Receiver;

use crate::traits::AudioStreamer;
use crate::types::SpeechEvent;

pub struct DummyAudioCapture;

impl DummyAudioCapture {
    pub fn new() -> Result<Self> {
        Err(anyhow!("Audio capture is not available in this build (missing 'backend-cpal' feature)"))
    }
}

pub struct DummyAudioPlayer;

impl DummyAudioPlayer {
    pub fn new() -> Result<Self> {
        Err(anyhow!("Audio playback is not available in this build (missing 'backend-cpal' feature)"))
    }
    
    pub fn play(&self, _samples: &[f32]) -> Result<()> {
        Err(anyhow!("Audio playback is not available"))
    }
}

pub struct DummyAudioStreamer;

impl DummyAudioStreamer {
    pub fn new() -> Result<Self> {
         Err(anyhow!("Audio streaming is not available in this build (missing 'backend-cpal' feature)"))
    }
}

impl AudioStreamer for DummyAudioStreamer {
    fn start_streaming(&mut self) -> Result<Receiver<SpeechEvent>> {
        Err(anyhow!("Audio streaming is not available"))
    }
}
