// AudioWorklet processor for capturing raw audio samples
// This runs in a separate audio processing thread

class AudioCaptureProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this._bufferSize = 4096; // ~256ms at 16kHz
    this._buffer = new Float32Array(this._bufferSize);
    this._bufferIndex = 0;
  }

  process(inputs, outputs, parameters) {
    const input = inputs[0];
    if (!input || !input[0]) {
      return true;
    }

    const samples = input[0]; // First channel

    // Accumulate samples into buffer
    for (let i = 0; i < samples.length; i++) {
      this._buffer[this._bufferIndex++] = samples[i];

      // When buffer is full, send to main thread
      if (this._bufferIndex >= this._bufferSize) {
        // Copy buffer data before sending
        const chunk = this._buffer.slice(0, this._bufferIndex);
        this.port.postMessage({ type: "audio", samples: chunk });
        this._bufferIndex = 0;
      }
    }

    return true; // Keep processor running
  }
}

registerProcessor("audio-capture-processor", AudioCaptureProcessor);
