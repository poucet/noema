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
