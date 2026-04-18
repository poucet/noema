# Simply Platform v1.0

**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
**Roadmap:** [ROADMAP.md](ROADMAP.md)
**Tasks:** [TASKS.md](TASKS.md)

---

## Problem

Lumina (Python Discord bot) and Noema (Rust desktop AI assistant) were converging on the same needs — LLM orchestration, MCP tools, voice pipeline, storage — but implemented independently in different languages.

## Solution

Unified Rust workspace: `simply-daemon` hub with `simply-core` internal library. Lumina is a Discord crate (serenity + songbird), Noema is a Tauri desktop client. Both connect to the daemon.

## What's Built

- **Daemon hub** — `simply-daemon` with API traits, axum REST + WS server, admin UI (Astro + Svelte 5)
- **API extraction** — `simply-daemon-api` subcrate: traits, types, `ToolProvider`, `Skill`, `RemoteDaemon`
- **Unified tool dispatch** — `ToolRegistry` with `ToolProvider` abstraction: MCP servers, WS clients, embedded skills all treated identically
- **RPC framework** — `simply-rpc` with `#[rpc_service]` proc macro, REST + WS dispatch, binary transfer
- **Discord bot** — serenity + songbird, chat, 15 MCP tools via rmcp, voice with DAVE encryption
- **Voice pipeline** — `simply-voice` with STT/TTS providers (Voxtral, Whisper, ElevenLabs, Gemini)
- **Content & RAG** — embedding pipeline (local/Ollama/Mistral/Gemini/Voyage), sqlite-vec, SearchApi, auto-RAG in Lumina
- **Google Docs** — `mcp-gdocs` skill with per-user OAuth, import with tabs + images
- **Multi-user auth** — daemon_secret, user scoping, admin OAuth, per-user MCP tokens (TransientTokenStore)
- **Transport layer** — HttpTransport for admin UI (REST + WS events)
- **Chat UI** — full conversation interface in admin UI with streaming, model selection

## What's Next

1. **Events & Intents** — event bus, action AST, LLM-compiled scheduled actions
2. **Web Extension** — Chrome extension as daemon client, Google Meet caption capture
3. **Multi-user polish** — persistent tokens, Discord role-based access, admin user management
