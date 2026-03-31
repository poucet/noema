import { useState, useRef, useCallback, useEffect } from "react";
import * as tauri from "../tauri";
import { audioLog } from "../utils/log";

interface AudioPlayerProps {
  data?: string; // base64 encoded audio data
  src?: string;  // URL to audio file (e.g., asset:// protocol)
  mimeType: string;
}

// Number of bars in the waveform visualization
const WAVEFORM_BARS = 50;

export function AudioPlayer({ data, src, mimeType }: AudioPlayerProps) {
  const [isPlaying, setIsPlaying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState(0);
  const [waveform, setWaveform] = useState<number[]>([]);
  const [duration, setDuration] = useState(0);
  const audioContextRef = useRef<AudioContext | null>(null);
  const audioBufferRef = useRef<AudioBuffer | null>(null);
  const sourceRef = useRef<AudioBufferSourceNode | null>(null);
  const startTimeRef = useRef<number>(0);
  const durationRef = useRef<number>(0);
  const animationRef = useRef<number | null>(null);

  // Helper to get audio bytes from either data or src
  const getAudioBytes = useCallback(async (): Promise<ArrayBuffer> => {
    if (data) {
      // Decode base64 to binary
      const binaryString = atob(data);
      const bytes = new Uint8Array(binaryString.length);
      for (let i = 0; i < binaryString.length; i++) {
        bytes[i] = binaryString.charCodeAt(i);
      }
      return bytes.buffer.slice(0);
    } else if (src) {
      // Fetch from URL
      const response = await fetch(src);
      return await response.arrayBuffer();
    }
    throw new Error("No audio data or src provided");
  }, [data, src]);

  // Decode audio and extract waveform on mount
  useEffect(() => {
    const extractWaveform = async () => {
      try {
        // Create a temporary audio context for decoding
        const audioContext = new AudioContext();

        const bytes = await getAudioBytes();

        // Decode audio data
        const audioBuffer = await audioContext.decodeAudioData(bytes);
        audioBufferRef.current = audioBuffer;
        setDuration(audioBuffer.duration);

        // Get raw audio samples (use first channel)
        const rawData = audioBuffer.getChannelData(0);
        const samplesPerBar = Math.floor(rawData.length / WAVEFORM_BARS);

        // Calculate RMS amplitude for each bar
        const bars: number[] = [];
        for (let i = 0; i < WAVEFORM_BARS; i++) {
          const start = i * samplesPerBar;
          const end = start + samplesPerBar;
          let sum = 0;
          for (let j = start; j < end && j < rawData.length; j++) {
            sum += rawData[j] * rawData[j];
          }
          const rms = Math.sqrt(sum / samplesPerBar);
          bars.push(rms);
        }

        // Normalize to 0-1 range
        const maxVal = Math.max(...bars, 0.01);
        const normalized = bars.map(v => v / maxVal);
        setWaveform(normalized);
        audioLog.debug("Waveform extracted", { bars: normalized.length, maxVal });

        // Close temporary context
        await audioContext.close();
      } catch (err) {
        audioLog.error("Failed to extract waveform", { err });
      }
    };

    extractWaveform();
  }, [data, src, getAudioBytes]);

  const stopPlayback = useCallback(() => {
    if (sourceRef.current) {
      try {
        sourceRef.current.stop();
      } catch {
        // Ignore errors if already stopped
      }
      sourceRef.current = null;
    }
    if (animationRef.current) {
      cancelAnimationFrame(animationRef.current);
      animationRef.current = null;
    }
    setIsPlaying(false);
    setProgress(0);
  }, []);

  const updateProgress = useCallback(() => {
    if (!audioContextRef.current || !isPlaying) return;

    const elapsed = audioContextRef.current.currentTime - startTimeRef.current;
    const progressPercent = Math.min((elapsed / durationRef.current) * 100, 100);
    setProgress(progressPercent);

    if (progressPercent < 100) {
      animationRef.current = requestAnimationFrame(updateProgress);
    }
  }, [isPlaying]);

  const play = useCallback(async () => {
    try {
      setError(null);

      // Stop any existing playback
      stopPlayback();

      // Create audio context if needed
      if (!audioContextRef.current) {
        audioContextRef.current = new AudioContext();
      }

      const audioContext = audioContextRef.current;

      // Resume context if suspended (browser autoplay policy)
      if (audioContext.state === "suspended") {
        await audioContext.resume();
      }

      // Get audio bytes from data or src
      const bytes = await getAudioBytes();

      // Decode audio data
      const audioBuffer = await audioContext.decodeAudioData(bytes);

      // Create and configure source node
      const source = audioContext.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(audioContext.destination);

      // Store for progress tracking
      sourceRef.current = source;
      startTimeRef.current = audioContext.currentTime;
      durationRef.current = audioBuffer.duration;

      // Handle playback end
      source.onended = () => {
        setIsPlaying(false);
        setProgress(100);
        sourceRef.current = null;
        if (animationRef.current) {
          cancelAnimationFrame(animationRef.current);
          animationRef.current = null;
        }
      };

      // Start playback
      source.start();
      setIsPlaying(true);

      // Start progress animation
      animationRef.current = requestAnimationFrame(updateProgress);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(`Failed to play audio: ${message}`);
      console.error("Audio playback error:", err);
    }
  }, [getAudioBytes, stopPlayback, updateProgress]);

  const toggle = useCallback(() => {
    if (isPlaying) {
      stopPlayback();
    } else {
      play();
    }
  }, [isPlaying, play, stopPlayback]);

  const handleDownload = useCallback(async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();

    // Generate filename from mime type
    const safeMimeType = mimeType || "audio/wav";
    const extension = safeMimeType.split("/")[1] || "bin";
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
    const filename = `audio-${timestamp}.${extension}`;

    audioLog.info("Download requested", { filename, mimeType: safeMimeType });

    try {
      if (data) {
        const result = await tauri.saveFile(data, filename, safeMimeType);
        audioLog.info("Download result", { result });
      } else if (src) {
        // For src URLs, fetch and convert to base64
        const response = await fetch(src);
        const blob = await response.blob();
        const reader = new FileReader();
        reader.onload = async () => {
          const base64 = (reader.result as string).split(",")[1];
          const result = await tauri.saveFile(base64, filename, safeMimeType);
          audioLog.info("Download result", { result });
        };
        reader.readAsDataURL(blob);
      }
    } catch (err) {
      audioLog.error("Failed to save file", { err });
    }
  }, [data, src, mimeType]);

  // Format duration as mm:ss
  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  const currentTime = (progress / 100) * duration;

  return (
    <div className="group flex items-center gap-3 p-3 bg-surface rounded-lg">
      <button
        onClick={toggle}
        className={`flex-shrink-0 w-10 h-10 rounded-full flex items-center justify-center transition-colors ${
          isPlaying
            ? "bg-red-500 hover:bg-red-600"
            : "bg-teal-600 hover:bg-teal-700"
        } text-white`}
        title={isPlaying ? "Stop" : "Play"}
      >
        {isPlaying ? (
          <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
            <rect x="6" y="4" width="4" height="16" />
            <rect x="14" y="4" width="4" height="16" />
          </svg>
        ) : (
          <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
            <polygon points="5,3 19,12 5,21" />
          </svg>
        )}
      </button>

      <div className="flex-1 flex flex-col gap-1 min-w-0">
        {/* Waveform visualization */}
        <div className="flex items-end h-8 w-full">
          {waveform.length > 0 ? (
            waveform.map((amplitude, i) => {
              // Bar is "played" if its position is before the current progress
              // Add 0.5 to center the threshold within each bar
              const barPosition = ((i + 0.5) / waveform.length) * 100;
              const isPlayed = progress > 0 && barPosition <= progress;
              // Minimum height of 4px, max of 32px (full height)
              const heightPx = Math.max(4, amplitude * 32);
              return (
                <div
                  key={i}
                  className={`transition-colors ${
                    isPlayed
                      ? "bg-teal-500"
                      : "bg-gray-500"
                  }`}
                  style={{
                    height: `${heightPx}px`,
                    width: `${100 / waveform.length}%`,
                    marginRight: i < waveform.length - 1 ? "1px" : "0",
                  }}
                />
              );
            })
          ) : (
            // Loading placeholder
            <div className="flex-1 flex items-center justify-center">
              <div className="text-xs text-muted">Loading...</div>
            </div>
          )}
        </div>

        {/* Time and info */}
        <div className="flex justify-between text-xs text-muted">
          <span>{formatTime(currentTime)} / {formatTime(duration)}</span>
          <span>{mimeType}</span>
        </div>
      </div>

      {error && (
        <div className="text-xs text-red-400" title={error}>
          ⚠️
        </div>
      )}

      {/* Download button */}
      <button
        onClick={handleDownload}
        className="flex-shrink-0 p-2 opacity-0 group-hover:opacity-100 hover:bg-elevated rounded-lg transition-all"
        title="Download audio"
      >
        <svg className="w-5 h-5 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
          />
        </svg>
      </button>
    </div>
  );
}
