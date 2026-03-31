# Voice Architecture

**Status:** Draft
**Version:** 1.0
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)

---

## Overview

Voice is split across three layers: provider abstraction, core pipeline orchestration, and platform-specific audio backends.

```
                  simply-voice (providers)
                  ├─ VoxtralProvider (STT + TTS)
                  ├─ (future: ElevenLabs, OpenAI, etc.)
                  │
┌─────────────────▼──────────────────┐
│         simply-core service         │
│  voice pipeline: VAD → STT → Agent → TTS  │
└──────┬──────────────────────┬──────┘
       │                      │
  simply-audio            lumina/voice
  (CPAL backend)         (songbird backend)
       │                      │
  Desktop mic/speaker    Discord voice channel
  (Noema)                (Lumina)
```

---

## Layers

- **simply-voice** defines provider traits (`SttProvider`, `TtsProvider`) and implementations
- **simply-core** orchestrates the pipeline (VAD → STT → agent → TTS)
- **Audio backends** are platform-specific: CPAL for desktop, songbird for Discord, WebRTC for /meet
- Each backend converts platform audio to/from the core's expected format (PCM)
