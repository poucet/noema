# Foundation Phase — Complete

This phase is done. The daemon is a working service with WS + REST, and Noema is wired to it.

---

## What was built

### Workspace structure
- `simply-core/` — LLM providers, MCP, agent orchestration (internal to daemon)
- `simply-core/llm/` — multi-provider LLM abstraction (Claude, OpenAI, Gemini, etc.)
- `simply-daemon/` — the hub: 7 API traits, EmbeddedDaemon, WS server, REST server, MCP registry
- `simply-rpc/` — generic trait-over-network RPC framework (proc macro + runtime)
- `simply-audio/` — audio capture (CPAL, browser) + whisper STT (to be replaced by simply-voice)
- `noema/` — Tauri desktop app, thin shim over daemon traits
- `config/` — shared config loading
- `noema-mcp-gdocs/` — Google Docs MCP server (sidecar, currently disabled)

### Daemon API traits (7 traits, all annotated with `#[rpc_service]`)
- **SessionApi** — session lifecycle, event streaming (`#[rpc(stream)]`)
- **ConversationApi** — persistent conversation CRUD
- **AssetApi** — binary asset storage (`#[rpc(base64_param/return)]`, `#[rpc(rest_get)]`)
- **McpApi** — MCP server registration, tool discovery
- **OAuthApi** — OAuth flow management
- **ModelApi** — model listing, provider info
- **VoiceApi** — voice pipeline (stub, `#[rpc(skip)]` on voice_connect)

### RPC framework (`simply-rpc`)
- `#[rpc_service("prefix")]` proc macro generates server dispatch + client macros
- `RpcService` trait with per-service `Stream` associated type
- `RpcClient` trait for network clients
- `Dispatcher` with HashMap prefix routing
- `ServiceMeta` with signature hashes for client/server compatibility
- REST auto-routing from `#[rpc(rest_get)]` metadata
- 43 tests

### Server architecture
- **WS server** (`ws/server.rs`) — generic, takes `DispatchFn`, knows nothing about APIs
- **REST server** (`ws/rest.rs`) — auto-routes from metadata, `/health`, `/kill`
- **RemoteDaemon** (`remote.rs`) — 7 one-liner macro invocations, public export
- **WsConnection** (`ws/client.rs`) — transport implementation (internal)
- **Discovery** (`ws/discovery.rs`) — `connect_or_host()` with caller-provided service builders
- **Service wiring** — in `main.rs` (standalone) and Noema's `init.rs`, not in ws/

---

## What's deferred

| Item | Deferred to | Notes |
|------|-------------|-------|
| DocumentApi (2.5) | Content phase | Blocked on sidecar orchestration design (see ACTIONS.md) |
| GDocs rewrite (2.5.1) | Content phase | Depends on DocumentApi |
| Peer registry (2.9) | Lumina phase | Needed when multiple clients connect |
| MCP client for sidecars (2.10) | Content phase | Needed for action service pattern |
| Voice pipeline | Voice phase | STT/TTS/VAD in daemon, simply-voice crate |

---

## Open design decisions (for future phases)

### Sidecar orchestration
Who connects fetch → store for domain features like GDocs? Three options documented in [ACTIONS.md](../../../designs/ACTIONS.md):
- A: Client orchestrates (simple, duplicates logic)
- B: Daemon orchestrates (clean for clients, daemon knows sources)
- C: Sidecar calls back via MCP (clean separation, bidirectional MCP)

### EmbeddedDaemon escape hatches
`EmbeddedDaemon` still exposes `stores()`, `coordinator()`, `mcp_service()` as pub accessors for gdocs commands. Remove once sidecar design is resolved.

### Connection lifecycle
- Client reconnection after flaky connections (connection ID + session resumption)
- WS disconnect cleanup (which server-side resources to keep vs drop)
- Ties into peer registry (task 2.9)
