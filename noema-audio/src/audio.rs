//! Audio capture and playback using cpal
//!
//! Provides:
//! - `AudioCapture` - Microphone input capture
//! - `AudioPlayback` - Speaker output
//! - `StreamingAudioCapture` - Real-time audio streaming with VAD
//! - `VoiceActivityDetector` - Detects speech start/end

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleFormat, SampleRate, StreamConfig};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

/// Convert samples to f32 format
fn convert_samples<T, F>(data: &[T], convert_fn: F) -> Vec<f32>
where
    T: Copy,
    F: Fn(T) -> f32,
{
    data.iter().map(|&sample| convert_fn(sample)).collect()
}

/// Resample audio to 16kHz for Whisper compatibility
pub fn resample_to_16khz(samples: &[f32], original_sample_rate: u32) -> Vec<f32> {
    if original_sample_rate == 16000 {
        return samples.to_vec();
    }

    let ratio = original_sample_rate as f32 / 16000.0;
    let output_len = (samples.len() as f32 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_index = (i as f32 * ratio) as usize;
        if src_index < samples.len() {
            output.push(samples[src_index]);
        } else {
            output.push(0.0);
        }
    }

    output
}

fn build_and_run_stream<T, F>(
    device: &Device,
    config: &StreamConfig,
    sender: Arc<Mutex<Sender<SpeechEvent>>>,
    vad: Arc<Mutex<VoiceActivityDetector>>,
    convert_fn: F,
) -> Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample + Send + 'static,
    F: Fn(T) -> f32 + Send + 'static,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let f32_data = convert_samples(data, &convert_fn);
            let mut vad_guard = vad.lock().unwrap();
            let sender_guard = sender.lock().unwrap();

            if let Some(event) = vad_guard.process_samples(&f32_data) {
                let _ = sender_guard.send(event);
            }
        },
        |err| eprintln!("Audio stream error: {}", err),
        None,
    )?;

    stream.play()?;
    Ok(stream)
}

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

/// Audio capture from default input device
pub struct AudioCapture {
    #[allow(dead_code)]
    host: Host,
    input_device: Device,
    config: StreamConfig,
}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let input_device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

        let supported_configs: Vec<_> = input_device.supported_input_configs()?.collect();

        let supported_config = supported_configs
            .iter()
            .filter(|c| c.channels() <= 2)
            .filter(|c| c.sample_format() == SampleFormat::F32)
            .next()
            .or_else(|| supported_configs.iter().filter(|c| c.channels() <= 2).next())
            .ok_or_else(|| anyhow::anyhow!("No supported audio input config found"))?;

        let desired_sample_rate = SampleRate(16000);
        let sample_rate = if supported_config.min_sample_rate() <= desired_sample_rate
            && desired_sample_rate <= supported_config.max_sample_rate()
        {
            desired_sample_rate
        } else {
            supported_config.min_sample_rate()
        };

        let config = StreamConfig {
            channels: std::cmp::min(1, supported_config.channels()),
            sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        Ok(Self {
            host,
            input_device,
            config,
        })
    }
}

/// Audio playback to default output device
pub struct AudioPlayback {
    #[allow(dead_code)]
    host: Host,
    output_device: Device,
    config: StreamConfig,
}

impl AudioPlayback {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let output_device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No output device available"))?;

        let supported_configs: Vec<_> = output_device.supported_output_configs()?.collect();
        let _supported_config = supported_configs
            .iter()
            .filter(|c| c.channels() <= 2)
            .next()
            .ok_or_else(|| anyhow::anyhow!("No supported audio output config found"))?;

        let sample_rate = SampleRate(16000);
        let config = StreamConfig {
            channels: 1,
            sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        Ok(Self {
            host,
            output_device,
            config,
        })
    }

    /// Play audio samples (expected to be 16kHz mono f32)
    pub fn play_samples(&self, samples: &[f32]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }

        let samples = Arc::new(samples.to_vec());
        let samples_clone = samples.clone();
        let sample_index = Arc::new(Mutex::new(0));
        let sample_index_clone = sample_index.clone();

        let stream = self.output_device.build_output_stream(
            &self.config,
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut index = sample_index_clone.lock().unwrap();
                for sample in output.iter_mut() {
                    if *index < samples_clone.len() {
                        *sample = samples_clone[*index];
                        *index += 1;
                    } else {
                        *sample = 0.0;
                    }
                }
            },
            |err| eprintln!("Audio playback error: {}", err),
            None,
        )?;

        stream.play()?;

        let duration_secs = samples.len() as f32 / self.config.sample_rate.0 as f32;
        std::thread::sleep(std::time::Duration::from_secs_f32(duration_secs + 0.1));

        Ok(())
    }
}

/// Handle to control the audio stream lifecycle
/// When dropped, signals the stream thread to stop
pub struct StreamHandle {
    stop_tx: Sender<()>,
    #[allow(dead_code)]
    thread: Option<JoinHandle<()>>,
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        // Signal the thread to stop
        let _ = self.stop_tx.send(());
    }
}

/// Streaming audio capture with voice activity detection
pub struct StreamingAudioCapture {
    audio_capture: AudioCapture,
    /// Handle to control stream lifetime - drop this to stop capture
    stream_handle: Option<StreamHandle>,
}

impl StreamingAudioCapture {
    pub fn new() -> Result<Self> {
        let audio_capture = AudioCapture::new()?;

        Ok(Self {
            audio_capture,
            stream_handle: None,
        })
    }

    /// Start streaming audio and return a receiver for speech events
    ///
    /// This spawns a dedicated thread to own the audio stream (cpal::Stream is !Send).
    /// The stream will stop when the StreamHandle is dropped.
    pub fn start_streaming(&mut self) -> Result<Receiver<SpeechEvent>> {
        let (event_sender, event_receiver) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

        let sample_rate = self.audio_capture.config.sample_rate.0;
        let config = self.audio_capture.config.clone();

        // Get device name to find it again in the thread
        let device_name = self.audio_capture.input_device
            .name()
            .unwrap_or_default();

        let thread = std::thread::spawn(move || {
            // Re-acquire device in this thread
            let host = cpal::default_host();
            let device = if device_name.is_empty() {
                host.default_input_device()
            } else {
                host.input_devices()
                    .ok()
                    .and_then(|mut devices| devices.find(|d| d.name().unwrap_or_default() == device_name))
                    .or_else(|| host.default_input_device())
            };

            let device = match device {
                Some(d) => d,
                None => {
                    let _ = ready_tx.send(Err("No audio input device".to_string()));
                    return;
                }
            };

            let supported_configs: Vec<_> = match device.supported_input_configs() {
                Ok(configs) => configs.collect(),
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("Failed to get audio configs: {}", e)));
                    return;
                }
            };

            let supported_config = match supported_configs.iter().filter(|c| c.channels() <= 2).next() {
                Some(c) => c,
                None => {
                    let _ = ready_tx.send(Err("No supported audio config".to_string()));
                    return;
                }
            };

            let sender = Arc::new(Mutex::new(event_sender));
            let vad = Arc::new(Mutex::new(VoiceActivityDetector::new(sample_rate)));

            // Copy the VAD settings
            {
                let mut vad_guard = vad.lock().unwrap();
                vad_guard.energy_threshold = 0.01;
                vad_guard.silence_duration_ms = 500;
                vad_guard.speech_duration_ms = 200;
            }

            let sample_format = supported_config.sample_format();

            macro_rules! handle_format {
                ($sample_type:ty, $converter:expr) => {
                    build_and_run_stream::<$sample_type, _>(
                        &device,
                        &config,
                        sender.clone(),
                        vad.clone(),
                        $converter,
                    )
                };
            }

            let stream_result = match sample_format {
                SampleFormat::I8 => handle_format!(i8, |sample| f32::from(sample) / i8::MAX as f32),
                SampleFormat::I16 => handle_format!(i16, |sample| f32::from(sample) / i16::MAX as f32),
                SampleFormat::I32 => handle_format!(i32, |sample| (sample as f32) / i32::MAX as f32),
                SampleFormat::I64 => handle_format!(i64, |sample| (sample as f32) / i64::MAX as f32),
                SampleFormat::U8 => handle_format!(u8, |sample| {
                    (f32::from(sample) - (1u8 << 7) as f32) / ((1u8 << 7) - 1) as f32
                }),
                SampleFormat::U16 => handle_format!(u16, |sample| {
                    (f32::from(sample) - (1u16 << 15) as f32) / ((1u16 << 15) - 1) as f32
                }),
                SampleFormat::U32 => handle_format!(u32, |sample| {
                    ((sample as f32) - (1u32 << 31) as f32) / ((1u32 << 31) - 1) as f32
                }),
                SampleFormat::U64 => handle_format!(u64, |sample| {
                    ((sample as f32) - (1u64 << 63) as f32) / ((1u64 << 63) - 1) as f32
                }),
                SampleFormat::F32 => handle_format!(f32, |sample| sample),
                SampleFormat::F64 => handle_format!(f64, |sample| sample as f32),
                _ => Err(anyhow::anyhow!("Unsupported sample format: {:?}", sample_format)),
            };

            let _stream = match stream_result {
                Ok(s) => {
                    let _ = ready_tx.send(Ok(()));
                    s
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };

            // Keep the stream alive until we receive a stop signal
            // The stream is owned by this thread and will be dropped when we exit
            let _ = stop_rx.recv();
        });

        // Wait for the stream to be ready
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(anyhow::anyhow!("{}", e)),
            Err(_) => return Err(anyhow::anyhow!("Audio thread failed to start")),
        }

        self.stream_handle = Some(StreamHandle {
            stop_tx,
            thread: Some(thread),
        });

        Ok(event_receiver)
    }
}

#[derive(Clone)]
struct VoiceActivityDetector {
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
    fn new(sample_rate_hz: u32) -> Self {
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

    fn process_samples(&mut self, samples: &[f32]) -> Option<SpeechEvent> {
        let energy = self.calculate_energy(samples);
        let is_speech = energy > self.energy_threshold;
        let now = Instant::now();
        let elapsed = now.duration_since(self.state_start_time);

        match self.current_state {
            VadState::Silence => {
                if is_speech {
                    self.transition_to(VadState::PossibleSpeech, now);
                    self.accumulated_audio.clear();
                    self.accumulated_audio.extend_from_slice(samples);
                }
                None
            }
            VadState::PossibleSpeech => {
                self.accumulated_audio.extend_from_slice(samples);

                if is_speech && elapsed.as_millis() >= self.speech_duration_ms as u128 {
                    self.transition_to(VadState::Speech, now);
                    Some(SpeechEvent::SpeechStart { timestamp: now })
                } else if !is_speech {
                    self.transition_to(VadState::Silence, now);
                    None
                } else {
                    None
                }
            }
            VadState::Speech => {
                self.accumulated_audio.extend_from_slice(samples);

                if !is_speech {
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
