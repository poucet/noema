//! Browser-based voice input backend
//!
//! Provides an `AudioStreamer` implementation that accepts audio pushed from
//! an external source (e.g., WebAudio from a browser).

use crate::traits::AudioStreamer;
use crate::types::SpeechEvent;
use crate::utils::resample_to_16khz;
use crate::vad::VoiceActivityDetector;
use anyhow::Result;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use tracing::warn;

/// Controller for pushing audio samples to the streamer
#[derive(Clone)]
pub struct BrowserAudioController {
    sender: Arc<Mutex<Option<Sender<SpeechEvent>>>>,
    vad: Arc<Mutex<VoiceActivityDetector>>,
    sample_rate: u32,
}

impl BrowserAudioController {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sender: Arc::new(Mutex::new(None)),
            vad: Arc::new(Mutex::new(VoiceActivityDetector::new(16000))), // Always processing at 16kHz
            sample_rate,
        }
    }

    /// Push audio samples (f32) to the VAD and streamer
    pub fn process_samples(&self, samples: &[f32]) {
        // Resample if necessary (Browser usually sends 16kHz or 44.1/48kHz)
        // For simplicity here, we assume the input might need resampling to 16kHz
        // But note: VAD expects 16kHz.
        
        let processed_samples = if self.sample_rate != 16000 {
            resample_to_16khz(samples, self.sample_rate)
        } else {
            samples.to_vec()
        };

        let mut vad = self.vad.lock().unwrap();
        if let Some(event) = vad.process_samples(&processed_samples) {
            let sender_guard = self.sender.lock().unwrap();
            if let Some(tx) = sender_guard.as_ref() {
                if let Err(e) = tx.send(event) {
                    warn!("Failed to send speech event from browser backend: {}", e);
                }
            }
        }
    }

    /// Force finish the current stream (flush VAD)
    /// This is useful when the user manually stops recording
    pub fn finish(&self) {
        // In a real implementation, we might want to force the VAD to flush 
        // any accumulated audio as a SpeechEnd event.
        // The current VAD implementation doesn't expose a "flush" but we can simulate
        // silence or add a method. For now, we'll just rely on the VAD state.
        // TODO: Implement explicit flush in VAD if needed.
    }
}

/// AudioStreamer implementation for browser audio
pub struct BrowserAudioStreamer {
    controller: BrowserAudioController,
}

impl BrowserAudioStreamer {
    pub fn new(controller: BrowserAudioController) -> Self {
        Self { controller }
    }
}

impl AudioStreamer for BrowserAudioStreamer {
    fn start_streaming(&mut self) -> Result<Receiver<SpeechEvent>> {
        let (tx, rx) = mpsc::channel();
        *self.controller.sender.lock().unwrap() = Some(tx);
        Ok(rx)
    }
}

/// Create a paired Controller and Streamer
pub fn create_browser_backend(sample_rate: u32) -> (BrowserAudioController, BrowserAudioStreamer) {
    let controller = BrowserAudioController::new(sample_rate);
    let streamer = BrowserAudioStreamer::new(controller.clone());
    (controller, streamer)
}
