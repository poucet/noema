import { useState, useRef, useCallback, useEffect } from "react";
import * as tauri from "../tauri";

export type VoiceStatus = "disabled" | "enabled" | "listening" | "transcribing";

interface UseVoiceInputOptions {
  onTranscription?: (text: string) => void;
  onError?: (error: string) => void;
}

export function useVoiceInput(options: UseVoiceInputOptions = {}) {
  const [status, setStatus] = useState<VoiceStatus>("disabled");
  const [isAvailable, setIsAvailable] = useState(false);
  const audioContextRef = useRef<AudioContext | null>(null);
  const processorRef = useRef<ScriptProcessorNode | null>(null);
  const streamRef = useRef<MediaStream | null>(null);

  // Check if voice is available (Whisper model exists on backend)
  useEffect(() => {
    tauri.isVoiceAvailable()
      .then(setIsAvailable)
      .catch(() => setIsAvailable(false));
  }, []);

  // Listen for transcription events from backend
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    tauri.onVoiceTranscription((text) => {
      setStatus("disabled");
      options.onTranscription?.(text);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onVoiceError((error) => {
      setStatus("disabled");
      options.onError?.(error);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onVoiceStatus((newStatus) => {
      if (newStatus === "listening") {
        setStatus("listening");
      } else if (newStatus === "transcribing") {
        setStatus("transcribing");
      } else if (newStatus === "disabled") {
        setStatus("disabled");
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [options.onTranscription, options.onError]);

  const startRecording = useCallback(async () => {
    try {
      // Request microphone access
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          sampleRate: 16000,
          echoCancellation: true,
          noiseSuppression: true,
        },
      });
      streamRef.current = stream;

      // Create audio context for raw PCM access
      const audioContext = new AudioContext({ sampleRate: 16000 });
      audioContextRef.current = audioContext;

      const source = audioContext.createMediaStreamSource(stream);

      // Use ScriptProcessorNode to get raw audio samples
      // Buffer size of 4096 at 16kHz = ~256ms chunks
      const processor = audioContext.createScriptProcessor(4096, 1, 1);
      processorRef.current = processor;

      processor.onaudioprocess = async (e) => {
        const inputData = e.inputBuffer.getChannelData(0);
        // Convert Float32Array to regular array for serialization
        const samples = Array.from(inputData);

        try {
          // Send audio chunk to backend
          await tauri.processAudioChunk(samples);
        } catch (err) {
          console.error("Failed to send audio chunk:", err);
        }
      };

      source.connect(processor);
      processor.connect(audioContext.destination);

      // Notify backend that recording started
      await tauri.startVoiceSession();
      setStatus("listening");
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      options.onError?.(message);
      setStatus("disabled");
    }
  }, [options.onError]);

  const stopRecording = useCallback(async () => {
    // Clean up audio resources
    if (processorRef.current) {
      processorRef.current.disconnect();
      processorRef.current = null;
    }
    if (audioContextRef.current) {
      await audioContextRef.current.close();
      audioContextRef.current = null;
    }
    if (streamRef.current) {
      streamRef.current.getTracks().forEach((track) => track.stop());
      streamRef.current = null;
    }

    try {
      // Notify backend that recording stopped
      setStatus("transcribing");
      await tauri.stopVoiceSession();
    } catch (err) {
      console.error("Failed to stop voice session:", err);
      setStatus("disabled");
    }
  }, []);

  const toggle = useCallback(async () => {
    if (status === "disabled") {
      await startRecording();
    } else if (status === "listening") {
      await stopRecording();
    }
    // Don't do anything if transcribing - wait for it to finish
  }, [status, startRecording, stopRecording]);

  return {
    status,
    isAvailable,
    toggle,
    startRecording,
    stopRecording,
  };
}
