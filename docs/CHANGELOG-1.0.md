# Simply Platform 1.0 Changelog

**Status:** In progress
**Started:** ~February 2026
**Commits:** 723+

Version 1.0 unifies Noema (Rust desktop) and Lumina (Python Discord bot) into a single Rust workspace with a shared daemon hub. Everything talks to `simply-daemon`.

---

## What's Next

See [v1.0/ROADMAP.md](v1.0/ROADMAP.md) and [v1.0/TASKS.md](v1.0/TASKS.md):
1. Content & RAG — document CRUD, embeddings, semantic search
2. Events & Intents — event bus, action AST, LLM-compiled intents
3. RTC — WebRTC voice, Google Meet integration
4. Multi-user & OAuth — identity, permissions, admin UI

---

## Docs Cleanup (2026-04-06)

Consolidated and cleaned up v1.0 documentation after completing Foundation, Lumina, and Voice phases.

- Retired `TODO.md` (manual test checklist) and `JOURNAL.md` (testing notes)
- Removed `phases/` directory — all 11 files across 6 subdirectories (foundation, lumina, voice, content, events, rtc)
- Consolidated into single [TASKS.md](v1.0/TASKS.md) with next phase tasks across 4 workstreams
- Rewrote [GOAL.md](v1.0/GOAL.md), [FEATURES.md](v1.0/FEATURES.md), and [ROADMAP.md](v1.0/ROADMAP.md) to reflect current state
- Moved [VOICE.md](designs/VOICE.md) from proposals to designs (implemented)
- Moved [AGENTIC.md](designs/proposals/AGENTIC.md) and [ACTIONS.md](designs/proposals/ACTIONS.md) into proposals (not yet built)
- Folded [TOOL_APPROVAL.md](designs/proposals/TOOL_APPROVAL.md) and [UCM_SERVICE.md](designs/proposals/UCM_SERVICE.md) into task plan
- Fleshed out next phase: Content & RAG, Events & Intents, RTC (Google Meet), Multi-user & OAuth

---

## Voice Phase

### Stage 1 — Voice Library

New `simply-voice` crate with provider abstraction.

- **`SttProvider` trait** — streaming speech-to-text
- **`TtsProvider` trait** — text-to-speech
- **`RealtimeProvider` trait** — bidirectional audio streaming
- **Voxtral** — STT + TTS via local MLX voice server (Apple Silicon) or Docker/vLLM
- **Whisper** — STT via OpenAI-compatible API
- **ElevenLabs** — TTS with voice selection and autocomplete
- **Gemini Realtime** — bidirectional audio streaming
- **VAD module** — voice activity detection
- **Audio types** — `Audio` with format metadata, PCM conversion utilities

### Stage 2 — Daemon Integration

- **STT stream** — `StreamHandle<VoiceInput, VoiceEvent>` via bidirectional WebSocket
- **TTS endpoint** — `POST /voice/tts`
- **`ServiceRouter`** — replaced `RestDispatcher`, cleaner routing
- **`RemoteXxxApi` structs** — replaced `impl_remote_xxx!` macros with generated client types
- **`RpcConnection` trait** — unified client connection abstraction
- **Voice provider registration** from `settings.toml`
- **Plaintext API keys** in settings (no env var requirement)
- **500 error retry** at protocol level for transient provider failures
- **Voice API hidden from LLM tools** — infrastructure, not agent-callable

### Stage 3 — Desktop Voice (Noema)

- CPAL mic capture -> daemon STT stream -> transcript into chat
- Auto-TTS via CPAL audio output (native, not webview Web Audio)
- Decoupled STT/TTS provider selection
- Voice provider + voice dropdown UI in settings

### Stage 4 — Discord Voice (Lumina)

See Lumina Stage 6 below — implemented as part of Lumina's voice commands.

---

## Lumina Phase

### Stage 1 — Discord Bot Crate

- `lumina` crate with serenity, connects to daemon via `RemoteDaemon`
- `#[slash_command]` + `#[command_group]` proc macros
- Guild-specific command registration, `.sync` owner command
- `LuminaContext` passed to all handlers (daemon + config access)

### Stage 2 — LLM Chat

- `/chat new` creates dedicated AI channels under configured category
- `on_message` listener responds in AI channels + @mentions
- Discord message history loaded as LLM context (configurable limit, paginated API)
- Streaming responses with progressive message edits
- `/chat pause`, `/chat resume` — per-channel toggle via channel topic tags
- `/chat model` — per-channel model selection with autocomplete from daemon
- Multimodal responses — images and audio as Discord attachments
- System prompt with conversation context instructions

### Stage 2.5 — Architecture Refactor

- `ToolAgent` + `SessionManager` + `AgentStreamSink` architecture
- Unified session management (ephemeral + persistent)
- Clean API separation between daemon and clients

### Stage 3 — Discord MCP Service

- Lumina registers as ephemeral MCP service with daemon on connect
- **15 Discord tools** via rmcp `#[tool]` macros: `list_channels`, `send_message`, `get_channel_history`, `search_messages`, `list_guilds`, `get_guild_info`, `list_members`, `get_member_info`, `list_roles`, `manage_roles`, `create_channel`, `manage_channel`, `pin_message`, `add_reaction`, `create_thread`
- `/tool call` — modal form generated from tool JSON schema, supports all content types
- `/tool list` — paginated embed of all registered MCP tools
- MCP instructions populated with guild/channel map (auto-refreshes on Discord events)
- Tool results with structured formatting and paginated embeds
- Daemon-side: `list_all_tools` + `call_tool_direct` using rmcp types natively
- Tool parameter schemas generated from RPC macro via `JsonSchema`
- Prefixed tool names: `discord.list_channels`, etc.

### Stage 6 — Discord Voice

- **Songbird** integration with DAVE encryption
- `/voice transcribe` — join channel, transcribe speech to text
- `/voice listen` — STT -> LLM session -> TTS -> play response in channel
- `/voice say` — text-to-speech playback in voice channel
- `/voice leave` — disconnect from voice channel
- `/voice list` — list available voices for current TTS provider
- `/voice status` — show current voice state
- `/voice provider` — switch STT/TTS provider with autocomplete
- `/voice set-voice` — select TTS voice with autocomplete
- Config persistence per guild (provider, voice selections)
- TTS fallback to text when synthesis fails
- Random voice selection when none configured
- Transcript routing to voice channel text chat
- WAV-in-memory for songbird (no temp files)
- Non-blocking audio receiver to prevent deadlocks

---

## Foundation Phase

### Stage 1 — Workspace Restructure

Renamed the workspace from `noema-*` to `simply-*` and established the crate boundaries.

- Renamed `noema-core` to `simply-core`, `noema-audio` to `simply-audio`
- Created `simply-daemon` crate with `DaemonApi` trait
- Merged `noema-mcp-core` into `simply-daemon/src/mcp/`
- Renamed `noema-desktop` to `noema`

### Stage 2 — Daemon Hub

Built `simply-daemon` as the central hub that all clients connect to.

- **`EmbeddedDaemon`** — in-process implementation for Noema desktop
- **`RemoteDaemon`** — WS + REST client for remote connections
- **WebSocket server** — rich client sessions with streaming
- **REST server** — asset management, health, admin
- **`simply-rpc` crate** — `#[rpc_service]` proc macro generates WS dispatch, REST routing metadata, and client code from trait definitions
- **Split DaemonApi** into focused traits: `SessionApi`, `ConversationApi`, `AssetApi`, `McpApi`, `OAuthApi`, `ModelApi`, `VoiceApi`, `CoreApi`
- **Smart discovery** — `connect_or_host()` tries remote, falls back to embedded
- **Auto-reconnect** — WS client with exponential backoff (100ms-30s)
- **Decoupled Noema** — all Noema Rust code imports from `simply-daemon` only, no `simply-core` dependency
- **`SessionManager`** — pluggable storage hooks, ephemeral and persistent sessions
- **`ToolService` trait** — `DaemonToolService` (REST methods as tools) + `CompositeToolService` (merged tool sources)

### Stage 3 — REST-First Transport

Upgraded the RPC framework to be REST-native with full HTTP semantics.

- **REST annotations** — `#[rpc(get = "/path")]`, `#[rpc(post = "/path")]`, etc.
- **`RestDispatcher`** — matchit-based URL routing with path params
- **`RouteMeta`** — `RouteKind::Rest(HttpMethod)` | `RouteKind::Stream`, cache control, binary response flags
- **Axum migration** — replaced hand-rolled hyper server
- **`BinaryResponse`** — typed binary returns with mime type and caching headers
- **`BinaryUpload`** — raw HTTP body upload with Content-Type
- **Single port** — merged WS and REST onto one axum server
- **Client codegen** — `impl_remote_xxx!` macros generate REST HTTP client code
- **Admin page** — active connections, sessions, models, kill endpoint
- **90 tests** — metadata, dispatch, round-trip, raw HTTP/WS, binary transfer

### Stage 4 — Service Extraction

Broke the monolithic `EmbeddedDaemon` into focused services.

- Extracted: `McpService`, `ModelService`, `AssetService`, `VoiceService`, `CoreService`
- `EmbeddedDaemon` delegates to services
- Services individually registered with `RestDispatcher`

### Stage F — Typed Content Dispatch

Added type-aware content transformation for MCP tool results.

- **`IntoContent` trait** — types declare how they become `ContentPart` (JSON or binary)
- **`FromContent` trait** — inverse direction for tool inputs (MCP image/audio -> `BinaryUpload`)
- **`#[derive(IntoContent)]`** — proc macro for all API return types
- **`rest_dispatch_as_content`** — RPC macro generates typed dispatch for tool invocation

---

## Infrastructure

### Local MLX Voice Server
- Python server for Apple Silicon TTS/STT via Voxtral
- Docker/vLLM alternative for NVIDIA
- Auto-detect in daemon startup script

### Developer Tooling
- `bin/daemon` script with start/stop/restart/status
- `bin/lumina` and `bin/noema` launcher scripts (run, split modes)
- Integration test harness with hurl + interactive mode
- File logging with rotation for daemon and Lumina
- `NOEMA_DATA_DIR` env override for test isolation

---

## Pre-1.0 Work (0.2 -> 1.0 transition)

Before the v1.0 restructure, significant work was done on Noema 0.2:

- Google Docs MCP server (`noema-mcp-gdocs`) with OAuth, import, and rendering
- Document panel with markdown rendering
- Model favorites and search/filter in selector
- Parallel model responses (SpanSet)
- Fork/regenerate/edit conversation features
- Document references in chat (`DocumentRef`, `DocumentResolver`)
- HTML-to-markdown paste
- Conversation privacy flag
- `StorageCoordinator` for coordinated multi-store operations
- Entity layer with `EntityStore`, relations, temporal queries
- Collections with items, fields, tags, schema hints
- Cross-references with backlinks
- UCM storage migration (Phases 1-3 of 0.2 roadmap)

---

## Highlights

- **Workspace unification** — single Rust workspace: `simply-core`, `simply-daemon`, `simply-rpc`, `simply-voice`, `noema`, `lumina`, `config`
- **Daemon hub** — `simply-daemon` with 8 API traits, axum server (REST + WS on single port), service extraction
- **RPC framework** — `simply-rpc` with `#[rpc_service]` proc macro, REST annotations, bidirectional streams, binary transfer
- **Discord bot** — full Lumina port: chat, 15 MCP tools, voice with DAVE encryption
- **Voice pipeline** — `simply-voice` crate with 4 providers, desktop + Discord voice, local MLX server

---

## Design Documents

- [designs/ARCHITECTURE.md](designs/ARCHITECTURE.md) — platform architecture
- [designs/CORE_SERVICE.md](designs/CORE_SERVICE.md) — daemon protocol (WS, REST, MCP)
- [designs/VOICE.md](designs/VOICE.md) — voice pipeline architecture
- [designs/UNIFIED_CONTENT_MODEL.md](designs/UNIFIED_CONTENT_MODEL.md) — UCM storage spec
- [designs/STORAGE.md](designs/STORAGE.md) — database schema
- [designs/proposals/](designs/proposals/) — proposals for events, actions, tool approval, UCM service
