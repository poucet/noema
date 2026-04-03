# Foundation Phase — Complete

This phase is done. The daemon is a REST-first service with WebSocket streaming, and all clients (Noema, Lumina) connect through `DaemonApi` traits.

---

## What was built

### Workspace structure
- `simply-core/` — LLM providers, MCP, agent orchestration (internal to daemon)
- `simply-core/llm/` — multi-provider LLM abstraction (Claude, OpenAI, Gemini, etc.)
- `simply-daemon/` — the hub: 8 API traits, service objects, axum server, MCP registry
- `simply-rpc/` — generic trait-over-network RPC framework (proc macro + runtime)
- `simply-audio/` — audio capture (CPAL, browser) + whisper STT (to be replaced by simply-voice)
- `noema/` — Tauri desktop app, thin shim over daemon traits
- `lumina/` — Discord bot, connects via RemoteDaemon
- `config/` — shared config loading
- `noema-mcp-gdocs/` — Google Docs MCP server (sidecar, currently disabled)

### Daemon API traits (8 traits, all annotated with `#[rpc_service]`)
- **SessionApi** — session lifecycle, event streaming (`#[rpc(stream = "/session/new")]`)
- **ConversationApi** — persistent conversation CRUD (`#[rpc(get/post/put/delete = "/conversation/...")]`)
- **AssetApi** — binary asset storage (`BinaryUpload`/`BinaryResponse` types, `immutable_cache`)
- **McpApi** — MCP server registration, tool discovery
- **OAuthApi** — OAuth flow management
- **ModelApi** — model listing, provider info, default model
- **VoiceApi** — voice pipeline (stub, `#[rpc(skip)]` on voice_connect)
- **DaemonInfoApi** — health check, kill, version

### RPC framework (`simply-rpc`)
- `#[rpc_service("prefix")]` proc macro generates server dispatch + client macros
- REST path annotations: `#[rpc(get = "/path/{param}")]`, `post`, `put`, `delete`
- Stream annotation: `#[rpc(stream = "/path")]` for WebSocket upgrade
- `RouteMeta` — compile-time metadata with `RouteKind`, path template, description, flags
- `RestDispatcher` with `matchit` crate for URL pattern matching
- `RestResult` carries `RouteMeta` for response encoding (binary, cache headers)
- `BinaryResponse` / `BinaryUpload` — native HTTP binary transfer (no base64)
- `WsConnection<E>` — generic WebSocket client with auto-reconnect
- `NotificationDemux<E>` — callback for routing WS notifications to streams
- Protocol types: `WsRequest`, `WsResponse`, `WsNotification`
- 90 tests across 4 test files

### Server architecture
- **Axum server** (`net/rest.rs`) — REST + admin on single port
- **WS server** (`net/server.rs`) — streaming-only (SessionApi), separate port for now
- **RemoteDaemon** (`remote.rs`) — reqwest for REST, WS for streams
- **Discovery** (`net/discovery.rs`) — `connect_or_host()` with caller-provided service builders

### Service objects (Stage 4)
- **McpService** — implements `McpApi` + `OAuthApi` directly
- **ModelService** — implements `ModelApi`
- **AssetService** — implements `AssetApi`
- **VoiceService** — implements `VoiceApi` (stub)
- **DaemonInfoService** — implements `DaemonInfoApi`
- **EmbeddedDaemon** — owns `SessionApi` + `ConversationApi`, delegates rest. Still implements `DaemonApi` for backward compat.
- **main.rs** registers individual services with `RestDispatcher`

### Tool system
- **DaemonToolService** — exposes REST methods as `ToolService` tools (from `RouteMeta`)
- **CompositeToolService** — merges multiple `ToolService` impls (MCP + daemon tools)

---

## What's deferred

| Item | Deferred to | Notes |
|------|-------------|-------|
| DocumentApi (2.5) | Content phase | Needs proper tab/revision/frontmatter model |
| GDocs rewrite (2.5.1) | Content phase | Depends on DocumentApi |
| Peer registry (2.9) | Lumina phase | Needed when multiple clients connect |
| MCP client for sidecars (2.10) | Content phase | Needed for action service pattern |
| Voice pipeline | Voice phase | STT/TTS/VAD in daemon, simply-voice crate |
| Single-port merge | Future | WS upgrade in axum handler (two ports for now) |
| RpcSchema / JSON Schema | Future | Tool input schemas are placeholder |
| Binary upload query params | Future | `#[rpc(post = "/asset?mime_type", body = raw)]` |
| Auth (remote access) | Post-v1 | REST binds to localhost only |

---

## Open design decisions (for future phases)

### Sidecar orchestration
Who connects fetch -> store for domain features like GDocs? Three options documented in [ACTIONS.md](../../../designs/ACTIONS.md):
- A: Client orchestrates (simple, duplicates logic)
- B: Daemon orchestrates (clean for clients, daemon knows sources)
- C: Sidecar calls back via MCP (clean separation, bidirectional MCP)

### MCP service registration from clients
Lumina (Stage 3) needs to register Discord tools with the daemon so other clients can use them. The daemon's `McpApi` currently only manages external MCP servers — need a path for clients to register as tool providers.

### Connection lifecycle
- Client reconnection after flaky connections (connection ID + session resumption)
- WS disconnect cleanup (which server-side resources to keep vs drop)
- Ties into peer registry (task 2.9)
