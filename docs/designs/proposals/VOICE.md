# Voice Architecture

**Status:** Draft
**Version:** 1.0
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)

---

## Overview

STT and TTS live in simply-daemon. Clients convert platform audio to a common format and stream it to the daemon. The daemon runs the full pipeline.

```
  Noema (CPAL)         Lumina (songbird)       Future: WebRTC
  mic → common fmt     Discord → common fmt    RTC → common fmt
       │                    │                       │
       └────────────────────┼───────────────────────┘
                            ▼
                    simply-daemon
                    ├─ simply-voice (providers)
                    │   ├─ VoxtralProvider (STT + TTS)
                    │   └─ (future: ElevenLabs, OpenAI, etc.)
                    │
                    └─ voice pipeline: VAD → STT → Agent → TTS
                            │
                            ▼
                    common fmt audio back to client
```

---

## Layers

- **simply-voice** — provider abstraction crate. Defines `SttProvider`, `TtsProvider` traits and implementations (Voxtral first). VAD module. Used internally by simply-daemon.
- **simply-daemon** — orchestrates the full pipeline (VAD → STT → agent → TTS). Exposes a voice streaming API to clients.
- **Clients** — convert platform-specific audio to/from a common format. They do NOT run STT/TTS locally.
  - **Noema**: CPAL captures desktop mic, converts to common format, streams to daemon
  - **Lumina**: songbird captures Discord audio, converts to common format, streams to daemon
  - **WebRTC**: browser/RTC audio, same pattern

## Common Audio Format

Clients and daemon agree on a common streaming format (PCM, specific sample rate/encoding). This is defined once and shared. Clients are responsible only for converting their platform audio to/from this format.

---

## Extensibility: RTC and Future Audio Sources

The voice API is generic — any process that can produce/consume the common audio format can stream to the daemon. This means future audio sources (WebRTC, Google Meet, phone bridge, etc.) don't need to be built into the daemon or any client.

An RTC service is just an external **action service** (see [CORE_SERVICE.md](CORE_SERVICE.md)):
1. Exposes MCP tools: `join_rtc(url)`, `leave_rtc()`
2. When called, connects to the RTC session and streams audio to/from the daemon's voice API
3. Registers event sources: `rtc.user_joined`, `rtc.user_left`

User says "join the meeting at https://..." → agent calls `join_rtc` → RTC service joins → audio flows through daemon's voice pipeline. No daemon changes needed.
