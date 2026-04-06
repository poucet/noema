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
| 1.7 | ✅ | ElevenLabs provider: `SttProvider` + `TtsProvider` | P0 | M |

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
| 2.7 | ✅ | Voice provider registration: Whisper, Voxtral, Gemini, ElevenLabs | P0 | S |
| 2.8 | ✅ | 500 error retry at protocol level (REST calls, 2 retries with backoff) | P0 | S |
| 2.9 | ✅ | Plaintext API keys in settings.toml (backward compat with encrypted) | P0 | S |
| 2.10 | ⬜ | Realtime mode: audio stream → RealtimeProvider (Gemini) | P1 | L |

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
| 4.1 | ✅ | Add songbird to Lumina (DAVE support, decode to mono 16kHz) | P0 | M |
| 4.2 | ✅ | `/voice transcribe` — join voice, transcribe speech to voice channel text | P0 | M |
| 4.3 | ✅ | `/voice listen` — join voice, STT → LLM session → TTS response, seeded with channel history | P0 | L |
| 4.4 | ✅ | `/voice say <text>` — TTS → play in voice channel (auto-join) | P0 | S |
| 4.5 | ✅ | `/voice leave` — leave voice channel, clean up session | P0 | S |
| 4.6 | ✅ | `/voice list` — show all providers and voices | P0 | S |
| 4.7 | ✅ | `/voice status` — show current STT/TTS provider + voice + session | P0 | S |
| 4.8 | ✅ | `/voice provider stt|tts <id>` — set provider (with autocomplete) | P0 | S |
| 4.9 | ✅ | `/voice set-voice <id>` — set TTS voice (with autocomplete) | P0 | S |
| 4.10 | ✅ | Voice config persistence in lumina.toml | P0 | S |
| 4.11 | ✅ | Graceful TTS fallback: 500 error → show text with 🔇 prefix | P0 | S |
| 4.12 | ✅ | Random voice selection when none configured | P1 | S |
| 4.13 | ⬜ | Hot-swap STT provider mid-session (without losing conversation) | P1 | M |
| 4.14 | ⬜ | Multi-user voice: distinguish speakers, turn-taking | P1 | M |
| 4.15 | ⬜ | `/voice set-voice` autocomplete showing ElevenLabs voices | P1 | S |

---

## Infrastructure (completed during voice work)

| Task | Notes |
|------|-------|
| ✅ Local MLX voice server (`bin/voice`) | Voxtral TTS + STT on Apple Silicon via mlx-audio |
| ✅ Docker/vLLM setup (`etc/vllm/`) | For NVIDIA GPU serving |
| ✅ Auto-detect local voice server in `bin/daemon` | Sets `VOXTRAL_BASE_URL` automatically |
| ✅ Voice API methods hidden from LLM tools (`no_tool`) | Prevents agent from calling TTS/STT directly |
| ✅ `WsConnection` simplified (no generic event type, unified sinks) | Cleaner WS client |
| ✅ `#[command_group]` macro: autocomplete support, `#[autocomplete]` param attr | Cleaner command definitions |
| ✅ `LuminaContext` reply helpers (`reply`, `reply_ephemeral`, `defer`) | DRY command responses |
| ✅ WAV-in-memory for songbird playback (symphonia wav+pcm codecs) | No temp files |
| ✅ Voice channel text chat for transcripts (not originating channel) | Cleaner UX |
