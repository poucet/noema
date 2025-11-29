import { useState, useRef, useCallback } from "react";

interface AudioPlayerProps {
  data: string; // base64 encoded audio data
  mimeType: string;
}

export function AudioPlayer({ data, mimeType }: AudioPlayerProps) {
  const [isPlaying, setIsPlaying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState(0);
  const audioContextRef = useRef<AudioContext | null>(null);
  const sourceRef = useRef<AudioBufferSourceNode | null>(null);
  const startTimeRef = useRef<number>(0);
  const durationRef = useRef<number>(0);
  const animationRef = useRef<number | null>(null);

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

      // Decode base64 to binary
      const binaryString = atob(data);
      const bytes = new Uint8Array(binaryString.length);
      for (let i = 0; i < binaryString.length; i++) {
        bytes[i] = binaryString.charCodeAt(i);
      }

      // Decode audio data
      const audioBuffer = await audioContext.decodeAudioData(bytes.buffer);

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
  }, [data, stopPlayback, updateProgress]);

  const toggle = useCallback(() => {
    if (isPlaying) {
      stopPlayback();
    } else {
      play();
    }
  }, [isPlaying, play, stopPlayback]);

  const handleDownload = useCallback(() => {
    const dataUrl = `data:${mimeType};base64,${data}`;
    const link = document.createElement("a");
    link.href = dataUrl;
    // Generate filename from mime type
    const extension = mimeType.split("/")[1] || "bin";
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
    link.download = `audio-${timestamp}.${extension}`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  }, [data, mimeType]);

  return (
    <div className="group flex items-center gap-2 p-2 bg-gray-100 dark:bg-gray-700 rounded-lg">
      <button
        onClick={toggle}
        className={`flex-shrink-0 w-10 h-10 rounded-full flex items-center justify-center transition-colors ${
          isPlaying
            ? "bg-red-500 hover:bg-red-600"
            : "bg-blue-500 hover:bg-blue-600"
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

      <div className="flex-1 flex flex-col gap-1">
        {/* Progress bar */}
        <div className="h-2 bg-gray-300 dark:bg-gray-600 rounded-full overflow-hidden">
          <div
            className="h-full bg-blue-500 transition-all duration-100"
            style={{ width: `${progress}%` }}
          />
        </div>

        {/* Info */}
        <div className="text-xs text-gray-500 dark:text-gray-400">
          {mimeType}
        </div>
      </div>

      {error && (
        <div className="text-xs text-red-500 dark:text-red-400" title={error}>
          ⚠️
        </div>
      )}

      {/* Download button */}
      <button
        onClick={handleDownload}
        className="flex-shrink-0 p-2 opacity-0 group-hover:opacity-100 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg transition-all"
        title="Download audio"
      >
        <svg className="w-5 h-5 text-gray-500 dark:text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
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
