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
| 1.2 | ⬜ | Voxtral provider: implements `SttProvider` + `TtsProvider` | P0 | M |
| 1.3 | ⬜ | Whisper provider: implements `SttProvider` | P1 | S |
| 1.4 | ⬜ | Gemini Realtime provider: implements `RealtimeProvider` | P0 | L |
| 1.5 | ⬜ | VAD module: voice activity detection | P0 | M |
| 1.6 | ⬜ | Audio format utilities: PCM conversion, sample rate | P0 | S |

---

## Stage 2 — Voice in Daemon

**Goal:** Voice pipeline runs in simply-daemon. Two modes: pipeline (STT → LLM → TTS) and realtime (bidirectional via RealtimeProvider).

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ⬜ | Pipeline mode: audio stream → VAD → STT → agent → TTS → audio back | P0 | L |
| 2.2 | ⬜ | Realtime mode: audio stream → VAD → RealtimeProvider (bidirectional) | P0 | L |
| 2.3 | ⬜ | WS binary frame multiplexing for audio transport | P0 | M |
| 2.4 | ⬜ | Pure STT mode: client sends audio, gets text back (no LLM) | P1 | S |
| 2.5 | ⬜ | Pure TTS mode: client sends text, gets audio back | P1 | S |

---

## Stage 3 — Desktop Voice (Noema)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.1 | ⬜ | Wire Noema: mic → daemon voice → transcript into chat conversation | P0 | M |
| 3.2 | ⬜ | Audio session management in Noema UI | P0 | S |
| 3.3 | ⬜ | Handle interruptions: user speaks while TTS is playing | P1 | S |

---

## Stage 4 — Discord Voice (Lumina)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 4.1 | ⬜ | Add songbird to Lumina crate | P0 | M |
| 4.2 | ⬜ | `/voice join`, `/voice leave`, `/voice converse` commands | P0 | M |
| 4.3 | ⬜ | Multi-user voice: distinguish speakers, turn-taking | P1 | M |
| 4.4 | ⬜ | DAVE encryption support | P1 | L |
