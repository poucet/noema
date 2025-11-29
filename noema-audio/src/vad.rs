use std::time::Instant;
use tracing::{debug, info};

use crate::types::{AudioSegment, SpeechEvent};
use crate::utils::resample_to_16khz;

#[derive(Clone)]
pub struct VoiceActivityDetector {
    energy_threshold: f32,
    silence_duration_ms: u64,
    speech_duration_ms: u64,
    current_state: VadState,
    state_start_time: Instant,
    accumulated_audio: Vec<f32>,
    sample_rate_hz: u32,
}

#[derive(Debug, Clone, PartialEq)]
enum VadState {
    Silence,
    PossibleSpeech,
    Speech,
    PossibleSilence,
}

impl VoiceActivityDetector {
    pub fn new(sample_rate_hz: u32) -> Self {
        Self {
            energy_threshold: 0.01,
            silence_duration_ms: 500,
            speech_duration_ms: 200,
            current_state: VadState::Silence,
            state_start_time: Instant::now(),
            accumulated_audio: Vec::new(),
            sample_rate_hz,
        }
    }

    pub fn process_samples(&mut self, samples: &[f32]) -> Option<SpeechEvent> {
        let energy = self.calculate_energy(samples);
        let is_speech = energy > self.energy_threshold;
        let now = Instant::now();
        let elapsed = now.duration_since(self.state_start_time);

        match self.current_state {
            VadState::Silence => {
                if is_speech {
                    debug!("VAD: Silence -> PossibleSpeech (energy: {:.4})", energy);
                    self.transition_to(VadState::PossibleSpeech, now);
                    self.accumulated_audio.clear();
                    self.accumulated_audio.extend_from_slice(samples);
                }
                None
            }
            VadState::PossibleSpeech => {
                self.accumulated_audio.extend_from_slice(samples);

                if is_speech && elapsed.as_millis() >= self.speech_duration_ms as u128 {
                    info!("VAD: PossibleSpeech -> Speech (confirmed speech start)");
                    self.transition_to(VadState::Speech, now);
                    Some(SpeechEvent::SpeechStart { timestamp: now })
                } else if !is_speech {
                    debug!("VAD: PossibleSpeech -> Silence (false positive)");
                    self.transition_to(VadState::Silence, now);
                    None
                } else {
                    None
                }
            }
            VadState::Speech => {
                self.accumulated_audio.extend_from_slice(samples);

                if !is_speech {
                    debug!("VAD: Speech -> PossibleSilence");
                    self.transition_to(VadState::PossibleSilence, now);
                }
                Some(SpeechEvent::SpeechChunk(AudioSegment::new(
                    now,
                    samples.to_vec(),
                )))
            }
            VadState::PossibleSilence => {
                self.accumulated_audio.extend_from_slice(samples);

                if is_speech {
                    debug!("VAD: PossibleSilence -> Speech (speech resumed)");
                    self.transition_to(VadState::Speech, now);
                    Some(SpeechEvent::SpeechChunk(AudioSegment::new(
                        now,
                        samples.to_vec(),
                    )))
                } else if elapsed.as_millis() >= self.silence_duration_ms as u128 {
                    let raw_audio = self.accumulated_audio.clone();
                    self.transition_to(VadState::Silence, now);
                    self.accumulated_audio.clear();

                    let audio_data = resample_to_16khz(&raw_audio, self.sample_rate_hz);
                    info!("VAD: PossibleSilence -> Silence (speech ended, {} samples)", audio_data.len());

                    Some(SpeechEvent::SpeechEnd(AudioSegment::new(now, audio_data)))
                } else {
                    None
                }
            }
        }
    }

    fn calculate_energy(&self, samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }

        let sum_squares: f32 = samples.iter().map(|&x| x * x).sum();
        (sum_squares / samples.len() as f32).sqrt()
    }

    fn transition_to(&mut self, new_state: VadState, timestamp: Instant) {
        self.current_state = new_state;
        self.state_start_time = timestamp;
    }
}
