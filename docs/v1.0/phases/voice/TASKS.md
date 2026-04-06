# Voice — Tasks

**Phase:** Voice
**Status:** In Progress
**Roadmap:** [ROADMAP.md](ROADMAP.md)
**Depends on:** Foundation (complete), Lumina (for Discord voice)

---

## Stage 1 — Voice Library

**Goal:** Standalone `simply-voice` crate with three provider traits and implementations.

**Three provider traits:**
- `SttProvider` — audio in → text out (Whisper, Voxtral)
- `TtsProvider` — text in → audio out (Voxtral)
- `RealtimeProvider` — audio in → audio out with built-in LLM (Gemini Realtime)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 1.1 | ✅ | Create `simply-voice/` crate with `SttProvider`, `TtsProvider`, `RealtimeProvider` traits | P0 | M |
| 1.2 | ✅ | Voxtral provider: implements `SttProvider` (no TTS available from Mistral) | P0 | M |
| 1.3 | ✅ | Whisper provider: implements `SttProvider` | P1 | S |
| 1.4 | ✅ | Gemini Realtime provider: implements `RealtimeProvider` | P0 | L |
| 1.5 | ✅ | VAD module: voice activity detection | P0 | M |
| 1.6 | ⬜ | Audio format utilities: PCM conversion, sample rate | P0 | S |

---

## Stage 2 — Voice in Daemon

**Goal:** Voice pipeline runs in simply-daemon. Two modes: pipeline (STT → LLM → TTS) and realtime (bidirectional via RealtimeProvider).

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ✅ | STT stream: audio → VAD → STT → UserTranscript back to client | P0 | L |
| 2.2 | ⬜ | Realtime mode: audio stream → VAD → RealtimeProvider (bidirectional) | P0 | L |
| 2.3 | ✅ | Bidi WS streams via StreamHandle (macro-generated) | P0 | M |
| 2.4 | ✅ | TTS endpoint: POST /voice/tts → synthesize text → AudioChunk | P0 | S |
| 2.5 | ⬜ | Voice provider selection UI + list endpoint | P1 | S |

---

## Stage 3 — Desktop Voice (Noema)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.1 | ✅ | Wire Noema: mic → daemon voice → transcript into chat conversation | P0 | M |
| 3.2 | ⬜ | Audio session management in Noema UI | P0 | S |
| 3.3 | ⬜ | Voice conversation mode: auto-TTS assistant responses + play through speakers | P0 | M |
| 3.4 | ⬜ | Handle interruptions: user speaks while TTS is playing | P1 | S |

---

## Stage 4 — Discord Voice (Lumina)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 4.1 | ⬜ | Add songbird to Lumina crate | P0 | M |
| 4.2 | ⬜ | `/voice transcribe` — join voice channel, transcribe all speech to text channel | P0 | M |
| 4.3 | ⬜ | `/voice listen` — join voice channel, STT → session → TTS response (full voice conversation) | P0 | L |
| 4.4 | ⬜ | `/voice say <text>` — speak text in the voice channel via TTS | P0 | S |
| 4.5 | ⬜ | `/voice leave` — leave voice channel | P0 | S |
| 4.6 | ⬜ | Multi-user voice: distinguish speakers, turn-taking | P1 | M |
| 4.7 | ⬜ | DAVE encryption support | P1 | L |
