# Voice — Tasks

**Phase:** Voice
**Status:** In Progress
**Roadmap:** [ROADMAP.md](ROADMAP.md)
**Depends on:** Foundation (complete), Lumina (for Discord voice)

---

## Stage 1 — Voice Library

**Goal:** Standalone `simply-voice` crate with provider traits and implementations.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 1.1 | ✅ | Create `simply-voice/` crate with `SttProvider`, `TtsProvider`, `RealtimeProvider` traits | P0 | M |
| 1.2 | ✅ | Voxtral provider: `SttProvider` + `TtsProvider` (Mistral API + local MLX) | P0 | M |
| 1.3 | ✅ | Whisper provider: `SttProvider` (local via whisper.cpp) | P1 | S |
| 1.4 | ✅ | Gemini Realtime provider: `RealtimeProvider` | P0 | L |
| 1.5 | ✅ | VAD module: voice activity detection | P0 | M |
| 1.6 | ✅ | Audio types: `AudioChunk`, `Audio` with `AudioFormat` (encoding + sample rate) | P0 | S |

---

## Stage 2 — Voice in Daemon

**Goal:** Voice pipeline in simply-daemon. STT/TTS as separate services, clients compose them.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ✅ | STT stream via `StreamHandle<VoiceInput, VoiceEvent>` (bidi WS) | P0 | L |
| 2.2 | ✅ | TTS endpoint: `POST /voice/tts` with voice selection | P0 | S |
| 2.3 | ✅ | Bidi WS streams via `StreamHandle` (macro-generated, generic) | P0 | M |
| 2.4 | ✅ | `ServiceRouter` replaces `RestDispatcher` + `ws_dispatch` (unified routing) | P0 | M |
| 2.5 | ✅ | `RemoteXxxApi` structs replace `impl_remote_xxx!` macros | P0 | M |
| 2.6 | ✅ | `RpcConnection` trait (object-safe, transport-agnostic) | P0 | M |
| 2.7 | ✅ | Voice provider registration: Whisper, Voxtral, Gemini (API keys + env vars) | P0 | S |
| 2.8 | ⬜ | Realtime mode: audio stream → RealtimeProvider (Gemini) | P1 | L |

---

## Stage 3 — Desktop Voice (Noema)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.1 | ✅ | Mic → daemon STT stream → transcript into chat conversation | P0 | M |
| 3.2 | ✅ | Auto-TTS: speak assistant responses via CPAL native audio | P0 | M |
| 3.3 | ✅ | Decoupled STT/TTS provider selection in Settings > Voice tab | P0 | M |
| 3.4 | ✅ | Voice provider + voice dropdown in settings UI | P0 | S |
| 3.5 | ⬜ | Persist voice settings in noema.toml | P1 | S |
| 3.6 | ⬜ | Handle interruptions: user speaks while TTS is playing | P1 | S |

---

## Stage 4 — Discord Voice (Lumina)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 4.1 | ⬜ | Add songbird to Lumina crate | P0 | M |
| 4.2 | ⬜ | `/voice transcribe` — join voice channel, transcribe all speech to text channel | P0 | M |
| 4.3 | ⬜ | `/voice listen` — join voice channel, STT → session → TTS response (voice conversation) | P0 | L |
| 4.4 | ⬜ | `/voice say <text>` — speak text in the voice channel via TTS | P0 | S |
| 4.5 | ⬜ | `/voice leave` — leave voice channel | P0 | S |
| 4.6 | ⬜ | Multi-user voice: distinguish speakers, turn-taking | P1 | M |
| 4.7 | ⬜ | DAVE encryption support | P1 | L |

---

## Infrastructure (completed during voice work)

| Task | Notes |
|------|-------|
| ✅ Local MLX voice server (`bin/voice`) | Voxtral TTS + STT on Apple Silicon via mlx-audio |
| ✅ Docker/vLLM setup (`etc/vllm/`) | For NVIDIA GPU serving |
| ✅ Auto-detect local voice server in `bin/daemon` | Sets `VOXTRAL_BASE_URL` automatically |
| ✅ Voice API methods hidden from LLM tools (`no_tool`) | Prevents agent from calling TTS/STT directly |
| ✅ `WsConnection` simplified (no generic event type, unified sinks) | Cleaner WS client |
