//! Speech-to-text transcription using Whisper

use anyhow::Result;
use std::path::Path;
use std::sync::Once;

static INIT_LOGGING: Once = Once::new();

/// Whisper-based speech transcriber
pub struct Transcriber {
    context: whisper_rs::WhisperContext,
}

impl Transcriber {
    /// Create a new transcriber with a Whisper model
    ///
    /// # Arguments
    /// * `model_path` - Path to the Whisper GGML model file (e.g., "ggml-base.en.bin")
    pub fn new(model_path: impl AsRef<Path>) -> Result<Self> {
        // Suppress whisper.cpp logging output (only runs once)
        INIT_LOGGING.call_once(|| {
            whisper_rs::install_logging_hooks();
        });

        let params = whisper_rs::WhisperContextParameters::default();
        let context = whisper_rs::WhisperContext::new_with_params(
            model_path.as_ref().to_str().unwrap(),
            params,
        )?;
        Ok(Self { context })
    }

    /// Transcribe audio samples to text
    ///
    /// # Arguments
    /// * `samples` - Audio samples at 16kHz mono f32 format
    pub fn transcribe(&self, samples: &[f32]) -> Result<String> {
        let mut state = self.context.create_state()?;
        let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });

        // Suppress all whisper output to avoid interfering with TUI
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);

        state.full(params, samples)?;

        let num_segments = state.full_n_segments();
        let mut result = String::new();

        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                result.push_str(&segment.to_string());
                result.push(' ');
            }
        }

        Ok(result.trim().to_string())
    }
}
