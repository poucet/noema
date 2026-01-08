import { useState, useRef, useCallback, useEffect } from "react";
import * as tauri from "../tauri";
import { voiceLog } from "../utils/log";

export type VoiceStatus = "disabled" | "enabled" | "listening" | "transcribing" | "buffering";

interface UseVoiceInputOptions {
  onTranscription?: (text: string) => void;
  onError?: (error: string) => void;
}

export function useVoiceInput(options: UseVoiceInputOptions = {}) {
  const [status, setStatus] = useState<VoiceStatus>("disabled");
  const [bufferedCount, setBufferedCount] = useState(0);
  const [isAvailable, setIsAvailable] = useState(false);
  const audioContextRef = useRef<AudioContext | null>(null);
  const workletNodeRef = useRef<AudioWorkletNode | null>(null);
  const streamRef = useRef<MediaStream | null>(null);

  // Use refs to avoid re-registering event listeners when callbacks change
  const onTranscriptionRef = useRef(options.onTranscription);
  const onErrorRef = useRef(options.onError);
  // Track last transcription to deduplicate
  const lastTranscriptionRef = useRef<string | null>(null);

  // Keep refs up to date
  useEffect(() => {
    onTranscriptionRef.current = options.onTranscription;
  }, [options.onTranscription]);

  useEffect(() => {
    onErrorRef.current = options.onError;
  }, [options.onError]);

  // Check if voice is available (Whisper model exists on backend)
  // Note: mediaDevices availability is checked at runtime when recording starts
  useEffect(() => {
    tauri.isVoiceAvailable()
      .then(setIsAvailable)
      .catch(() => setIsAvailable(false));
  }, []);

  // Listen for transcription events from backend - only register once
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    tauri.onVoiceTranscription((text) => {
      // Deduplicate - skip if we just received this exact transcription
      if (text === lastTranscriptionRef.current) {
        voiceLog.warn("Duplicate transcription skipped", { text });
        return;
      }
      lastTranscriptionRef.current = text;
      voiceLog.info("Transcription received", { text });
      setStatus("disabled");
      onTranscriptionRef.current?.(text);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onVoiceError((error) => {
      voiceLog.error("Voice error", { error });
      setStatus("disabled");
      onErrorRef.current?.(error);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onVoiceStatus((newStatus) => {
      voiceLog.debug("Voice status changed", { newStatus });
      if (newStatus === "listening") {
        setStatus("listening");
        setBufferedCount(0);
      } else if (newStatus === "transcribing") {
        setStatus("transcribing");
      } else if (newStatus === "disabled") {
        setStatus("disabled");
        setBufferedCount(0);
      } else if (newStatus.startsWith("buffering:")) {
        const count = parseInt(newStatus.split(":")[1], 10) || 0;
        setStatus("buffering");
        setBufferedCount(count);
      } else if (newStatus === "enabled") {
        setStatus("enabled");
        setBufferedCount(0);
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []); // Empty deps - only register once

  const startRecording = useCallback(async () => {
    voiceLog.info("Starting recording");
    // Clear dedup ref for new recording session
    lastTranscriptionRef.current = null;
    try {
      // Check if mediaDevices API is available (requires secure context)
      if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
        throw new Error(
          "Microphone access requires a secure context (HTTPS). " +
          "Voice input is not available in this environment."
        );
      }

      // Request microphone access
      voiceLog.debug("Requesting microphone access");
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          sampleRate: 16000,
          echoCancellation: true,
          noiseSuppression: true,
        },
      });
      streamRef.current = stream;
      voiceLog.debug("Microphone access granted");

      // Create audio context for raw PCM access
      const audioContext = new AudioContext({ sampleRate: 16000 });
      audioContextRef.current = audioContext;

      // Load AudioWorklet processor module
      voiceLog.debug("Loading AudioWorklet processor");
      await audioContext.audioWorklet.addModule("/audio-processor.js");

      const source = audioContext.createMediaStreamSource(stream);

      // Create AudioWorkletNode (modern replacement for ScriptProcessorNode)
      const workletNode = new AudioWorkletNode(audioContext, "audio-capture-processor");
      workletNodeRef.current = workletNode;

      // Handle audio chunks from the worklet
      workletNode.port.onmessage = async (event) => {
        if (event.data.type === "audio") {
          const samples = Array.from(event.data.samples as Float32Array);
          try {
            await tauri.processAudioChunk(samples);
          } catch (err) {
            voiceLog.error("Failed to send audio chunk", { err });
          }
        }
      };

      source.connect(workletNode);
      // AudioWorklet doesn't require connection to destination to run,
      // but we connect to a muted gain node for consistency
      const gainNode = audioContext.createGain();
      gainNode.gain.value = 0;
      workletNode.connect(gainNode);
      gainNode.connect(audioContext.destination);

      // Notify backend that recording started
      await tauri.startVoiceSession();
      voiceLog.info("Recording started successfully");
      setStatus("listening");
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      voiceLog.error("Failed to start recording", { error: message });
      onErrorRef.current?.(message);
      setStatus("disabled");
    }
  }, []);

  const stopRecording = useCallback(async () => {
    voiceLog.info("Stopping recording");
    // Clean up audio resources
    if (workletNodeRef.current) {
      workletNodeRef.current.disconnect();
      workletNodeRef.current = null;
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
      voiceLog.debug("Waiting for transcription");
      await tauri.stopVoiceSession();
    } catch (err) {
      voiceLog.error("Failed to stop voice session", { err });
      setStatus("disabled");
    }
  }, []);

  const toggle = useCallback(async () => {
    if (status === "disabled") {
      await startRecording();
    } else if (status === "listening" || status === "enabled" || status === "buffering") {
      // Allow stopping in listening, enabled (idle during thinking), or buffering states
      await stopRecording();
    }
    // Don't do anything if transcribing - wait for it to finish
  }, [status, startRecording, stopRecording]);

  return {
    status,
    bufferedCount,
    isAvailable,
    toggle,
    startRecording,
    stopRecording,
  };
}
