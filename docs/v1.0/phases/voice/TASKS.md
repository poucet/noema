# Voice — Tasks

**Phase:** Voice
**Status:** Not Started
**Roadmap:** [ROADMAP.md](ROADMAP.md)
**Depends on:** Foundation (complete), Lumina (for Discord voice)

---

## Stage 1 — Voice Library

**Goal:** Standalone `simply-voice` crate with provider traits and at least one implementation.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 1.1 | ⬜ | Create `simply-voice/` crate with STT/TTS provider traits | P0 | M |
| 1.2 | ⬜ | Voxtral (Mistral) provider implementation | P0 | M |
| 1.3 | ⬜ | Whisper provider (extract from current simply-audio) | P1 | S |
| 1.4 | ⬜ | VAD module: voice activity detection | P0 | M |
| 1.5 | ⬜ | Audio format utilities: PCM conversion, sample rate | P0 | S |

---

## Stage 2 — Voice in Daemon

**Goal:** Voice pipeline runs in simply-daemon. Clients stream audio, daemon handles STT/TTS.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ⬜ | VoiceApi implementation: audio stream → VAD → STT → text events | P0 | L |
| 2.2 | ⬜ | TTS pipeline: text → audio stream back to client | P0 | M |
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
