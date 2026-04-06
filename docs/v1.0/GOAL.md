# Design 1: Simply Platform — Rust Unification

**Status**: in progress
**Features:** [FEATURES.md](FEATURES.md)
**Roadmap:** [ROADMAP.md](ROADMAP.md)
**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)

---

## Problem

Lumina (Python Discord bot) and Noema (Rust desktop AI assistant) were converging on the same needs — LLM orchestration, MCP tools, voice pipeline, storage — but implemented independently in different languages.

## Solution

Unified Rust workspace: `simply-daemon` hub with `simply-core` internal library. Lumina is a Discord crate (serenity + songbird), Noema is a Tauri desktop client. Both connect to the daemon.

Three interfaces: WebSocket + JSON for rich clients, REST for triggers/management, MCP outbound for action services.

## Non-goals (v1)

- Telegram, WhatsApp, or other messaging platform integrations
- Full feature parity with Python Lumina

---

## What's Built

- **Workspace restructured** — `simply-core`, `simply-daemon`, `simply-rpc`, `simply-voice`, `noema`, `lumina`, `config`
- **Daemon hub** — 8 API traits, axum REST + WS server, service extraction
- **RPC framework** — `#[rpc_service]` proc macro, REST dispatch, binary transfer, `ServiceRouter`
- **Typed content** — `IntoContent`/`FromContent` traits, `rest_dispatch_as_content` macro
- **Discord bot** — serenity-based, chat (channel management, streaming, model selection), 15 MCP tools via rmcp, `/tool call` + `/tool list`
- **Voice pipeline** — `simply-voice` crate with STT/TTS providers (Voxtral, Whisper, ElevenLabs, Gemini), VAD, daemon integration
- **Desktop voice** — mic capture via CPAL, daemon STT/TTS, provider/voice selection UI
- **Discord voice** — songbird with DAVE encryption, `/voice` commands (transcribe, listen, say, leave, list, status, provider, set-voice), config persistence, TTS fallback, transcript routing

## What's Next

See [ROADMAP.md](ROADMAP.md) and [TASKS.md](TASKS.md) for the next phase:
1. **Content & RAG** — document CRUD, embedding providers, semantic search
2. **Events** — event bus, intents, scheduled actions
3. **RTC** — WebRTC voice service, Google Meet integration
4. **Multi-user & OAuth** — per-user identity, Google account linking, permission model, admin UI

---

## Related

- Architecture: [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
- Post-v1 Roadmap: [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)
