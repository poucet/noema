# Simply Platform v1.0 — Roadmap

**Design:** [GOAL.md](GOAL.md)
**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
**Post-v1:** [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)
**Tasks:** [TASKS.md](TASKS.md)

---

## Completed

### Foundation
Workspace restructure, daemon hub, REST-first transport, service extraction, typed content dispatch.

- `simply-daemon` hub with 8 API traits, axum REST + WS server
- `simply-rpc` framework: `#[rpc_service]` proc macro, `ServiceRouter`, binary transfer
- `IntoContent`/`FromContent` traits, `rest_dispatch_as_content` macro
- Service extraction: McpService, ModelService, AssetService, VoiceService, DaemonInfoService
- `RpcConnection` trait, `RemoteXxxApi` client structs

### Lumina (Discord Bot)
Discord bot with LLM chat, MCP tool infrastructure, and full voice support.

- serenity-based bot connecting via RemoteDaemon
- LLM chat: channel management, streaming, model selection, pause/resume
- MCP service: 15 Discord tools via rmcp, ephemeral registration
- `/tool call` (modal) + `/tool list` (paginated embed)
- Dynamic channel map in MCP instructions
- Voice: songbird + DAVE, transcribe/listen/say/leave/list/status, provider selection, config persistence, TTS fallback, transcript routing

### Voice
Voice pipeline for desktop and Discord.

- `simply-voice` crate: STT (Voxtral, Whisper), TTS (Voxtral, ElevenLabs), Realtime (Gemini), VAD
- Daemon integration: STT bidi WS stream, TTS endpoint, provider registration
- Desktop: CPAL mic capture, auto-TTS, provider/voice dropdown UI
- Discord: all voice commands, DAVE encryption

### Known Deferred Items
- Realtime mode (Gemini audio-in/audio-out through daemon)
- Hot-swap STT provider mid-stream
- Multi-user voice (multiple speakers in one channel)
- ElevenLabs voice autocomplete
- Persist desktop voice settings
- Handle voice interruptions
- Lumina Stage 3.5 verification (Noema using Discord tools through daemon)
- Simplify chat storage: chat messages currently go through content blocks (immutable text with origin tracking), but chat messages are never re-referenced, forked, or organized — the indirection buys little. Consider storing chat turn text inline in the turns/messages table instead, removing the content block indirection for chat. Doesn't affect documents, which genuinely benefit from the content block model.
- Frontmatter-aware search filtering: extend SearchApi to filter by arbitrary frontmatter key-value conditions (e.g. `tags contains "urgent"`, `due < 2026-05-01`). Needs a well-defined filter syntax and efficient query strategy (parse YAML only on narrowed result sets after type/user pre-filtering).
- Split `simply-daemon` into `simply-daemon-api` (lightweight: API traits, types, RemoteDaemon, DaemonSession, WS client) and `simply-daemon` (heavy: EmbeddedDaemon, services, storage, LLM, voice, embeddings). Lumina and Noema would depend only on `simply-daemon-api` for much faster builds when running against a remote daemon.

---

## Next Phase

Four parallel workstreams. See [TASKS.md](TASKS.md) for detailed tasks.

### 1. Content & RAG
Document CRUD with frontmatter conventions, embedding providers, semantic search over all UCM content. Foundation for everything that needs searchable knowledge.

### 2. Events & Intents
Reactive event system — timers, platform events (Discord, desktop), LLM-compiled intents with action ASTs. Scheduled prompts, automated workflows.

### 3. Web Extension & RTC ⏸️
Chrome extension (`simply-web`) as a daemon client — chat, MCP tools, content capture from any webpage. Meeting transcription via Google Meet caption scraping. Audio streaming and full RTC participation deferred until Google Meet Media API goes GA. Deprioritized for now.

### 4. Multi-user & OAuth
Per-user identity with OAuth. Different Discord users link different Google accounts — affects their MCP tool access. Role-based permission model for MCP tools (starts with Discord roles, generalizes). Admin web UI with login for remote hosting.

---

## Parallelization

```
Completed ─────────────────────────────────────────────────

Foundation  ████████████████████  (done)
Lumina      ████████████████████  (done — stages 1-3, 6)
Voice       ████████████████████  (done — stages 1-4)

Next ──────────────────────────────────────────────────────

Content & RAG     ██████████████████████████
Events & Intents  ██████████████████████████████████████
RTC               ████████████████████  (after Content stage 1)
Multi-user/OAuth  ██████████████████████████████████████
```

Content & RAG, Events, and Multi-user can start independently. RTC depends on the daemon's MCP service registration (Content stage 1) for its `join_rtc` / `leave_rtc` tools.
