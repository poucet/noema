# Simply Platform v1.0 — Roadmap

**Design:** [GOAL.md](GOAL.md)
**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
**Post-v1:** [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)
**Tasks:** [TASKS.md](TASKS.md)

---

## Completed

### Foundation
Workspace restructure, daemon hub, REST-first transport, service extraction, typed content dispatch.

### Lumina (Discord Bot)
serenity-based bot with LLM chat, 15 MCP Discord tools, voice (songbird + DAVE).

### Voice
`simply-voice` crate with STT/TTS providers (Voxtral, Whisper, ElevenLabs, Gemini), daemon integration, desktop + Discord voice.

### Content & RAG
Embedding providers (local/Ollama/Mistral/Gemini/Voyage), sqlite-vec vector store, SearchApi, Lumina auto-RAG, Google Docs import with per-user OAuth.

### Unified Frontend & API Extraction
- Admin UI (Astro + Svelte 5) with chat, settings, Google Docs import, MCP management
- `simply-daemon-api` subcrate (API traits, ToolProvider, Skill, RemoteDaemon)
- Transport abstraction layer (HttpTransport with REST + WS events)
- `ToolRegistry` replacing `CompositeToolService` — unified `ToolProvider` dispatch

### Multi-user Auth (Stage 1-2)
daemon_secret, X-User-Id, admin OAuth login, user self-service auth, Discord user mapping.

---

## Next Phase

### 1. UCM Unification
Collapse documents, tabs, revisions onto the `entities` + `entity_relations` + `content_blocks` substrate. Drop the `DocumentStore` trait; daemon exposes a generic `EntityApi`. Admin and Noema UIs become entity-first and render per entity capability (has content? has `contained_in` children? has revisions?). The only place that still encodes "a Google Doc becomes a `document::tabbed` with child `document_tab` entities" is the import skill. Adds per-source frontmatter and entity-type filtering to RAG. Unlocks directories, knowledge graph, labels, and any future composition with no further schema work. **Design:** [UNIFIED_CONTENT_MODEL.md](../designs/UNIFIED_CONTENT_MODEL.md). Resolves the "Remove CASCADE reliance from SQLite schema" deferred item by moving cascade logic into the orchestrator.

Ships non-breaking: each commit leaves daemon, Lumina, and Noema running. Fast-follow milestone adds directory filing UX + drag-and-drop in both admin and Noema once the core is in.

### 2. Events & Intents
Reactive event system — timers, platform events (Discord, desktop), LLM-compiled intents with action ASTs. Scheduled prompts, automated workflows.

### 3. Web Extension & RTC (paused)
Chrome extension as daemon client — chat, MCP tools, content capture. Google Meet caption scraping. Deprioritized for now.

### 4. Multi-user Polish
Per-user per-MCP-server persistent tokens, Discord role-based access control, admin user management UI.

---

## Known Deferred Items

- Realtime voice mode (Gemini audio-in/audio-out)
- Hot-swap STT provider mid-stream
- Multi-user voice (multiple speakers in one channel)
- Simplify chat storage: consider storing chat turn text inline instead of through content blocks
- Frontmatter-aware search filtering (arbitrary key-value conditions)
- Persist OAuth tokens across daemon restarts (encrypt-at-rest or refresh tokens)
- OAuth identity linking not working end-to-end (Discord→email user merge)
- Document visibility (is_public flag exists but unused)
- Noema search panel + document refs (depends on UI work)
