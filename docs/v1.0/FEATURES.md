# v1.0 Feature Inventory

**Design:** [GOAL.md](GOAL.md)
**Roadmap:** [ROADMAP.md](ROADMAP.md)

---

## Built

### simply-core (shared services)

| Feature | Status | Notes |
|---------|--------|-------|
| Agent orchestration | ✅ | ToolAgent, SessionManager, spawn_agent |
| LLM providers | ✅ | Claude, OpenAI, Gemini, Ollama |
| MCP server/client | ✅ | McpRegistry, ephemeral registration, rmcp |

### simply-rpc (RPC framework)

| Feature | Status | Notes |
|---------|--------|-------|
| `#[rpc_service]` proc macro | ✅ | REST path annotations, stream annotations |
| `ServiceRouter` | ✅ | Replaced RestDispatcher, matchit URL routing |
| `RpcConnection` trait | ✅ | Unified client connection abstraction |
| Binary transfer | ✅ | `BinaryResponse` + `BinaryUpload` |
| Typed content dispatch | ✅ | `IntoContent`/`FromContent`, `rest_dispatch_as_content` |

### simply-voice (voice pipeline)

| Feature | Status | Notes |
|---------|--------|-------|
| STT providers | ✅ | Voxtral, Whisper |
| TTS providers | ✅ | Voxtral, ElevenLabs |
| Realtime provider | ✅ | Gemini |
| VAD | ✅ | Voice activity detection module |
| Daemon integration | ✅ | STT stream via bidi WS, TTS endpoint, provider registration |

### simply-daemon (hub)

| Feature | Status | Notes |
|---------|--------|-------|
| 8 API traits | ✅ | Session, Conversation, Asset, Mcp, OAuth, Model, Voice, DaemonInfo |
| Axum REST + WS server | ✅ | Single port, REST + WS |
| Service extraction | ✅ | McpService, ModelService, AssetService, VoiceService, DaemonInfoService |
| `EmbeddedDaemon` | ✅ | In-process for Noema |
| `RemoteDaemon` → `RemoteXxxApi` | ✅ | WS + HTTP client structs |
| 500 error retry | ✅ | Protocol-level retry |
| Plaintext API keys | ✅ | In settings.toml |

### lumina (Discord bot)

| Feature | Status | Notes |
|---------|--------|-------|
| Discord gateway | ✅ | serenity-based, connects via RemoteDaemon |
| Chat commands | ✅ | Channel management, streaming, model selection, pause/resume |
| MCP service | ✅ | 15 Discord tools, ephemeral registration |
| `/tool call` + `/tool list` | ✅ | Modal form from schema, paginated embed |
| MCP instructions | ✅ | Dynamic channel map, refreshes on Discord events |
| `.sync` command | ✅ | Owner command sync |
| Voice I/O | ✅ | Songbird + DAVE encryption |
| Voice commands | ✅ | transcribe, listen, say, leave, list, status, provider, set-voice |
| Voice config | ✅ | Config persistence, TTS fallback, random voice, transcript routing |

### noema (Tauri desktop)

| Feature | Status | Notes |
|---------|--------|-------|
| Desktop voice | ✅ | CPAL mic capture, daemon STT/TTS |
| Voice UI | ✅ | Provider/voice dropdown, decoupled STT/TTS selection |

---

## Next Phase

| Feature | Priority | Area | Notes |
|---------|----------|------|-------|
| Document CRUD | P0 | Content | ⬜ DocumentApi, frontmatter queries, named documents |
| Embedding providers | P0 | Content | ⬜ Vector storage, embedding API trait |
| RAG pipeline | P0 | Content | ⬜ Query -> embed -> search -> inject context -> LLM |
| Event bus | P1 | Events | ⬜ Pub/sub, typed payloads, timer sources |
| Intent engine | P1 | Events | ⬜ Action AST, LLM-compiled intents |
| Web extension | P2 | Web | ⏸️ Chrome extension daemon client, Meet caption capture |
| Multi-user identity | P0 | Auth | ⬜ Per-user OAuth, Discord user -> Google account linking |
| Permission model | P0 | Auth | ⬜ Role-based MCP tool access, generalizes beyond Discord |
| Admin UI | P1 | Auth | ⬜ Web UI for all REST APIs, login for remote hosting |

---

## Deferred (post-v1)

| Feature | Reason |
|---------|--------|
| Telegram/WhatsApp | New presentation layer crates |
| Image generation | MCP tool server, low priority |
| PDF extraction / video transcription | Multimodal pipeline features |
| Wiki cross-linking | Depends on content phase |
| Hierarchical tags | Depends on content phase |
