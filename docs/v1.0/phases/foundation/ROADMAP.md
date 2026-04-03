# Foundation

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 — everything else depends on this.
**Status:** Complete
**Tasks:** [TASKS.md](TASKS.md)
**Handoff:** [HANDOFF.md](HANDOFF.md)

---

## Goal

Restructure the workspace to match the target architecture and build the daemon hub that all clients connect to.

- **simply-core** — library crate, internal to simply-daemon. LLM providers, MCP server/client, agent orchestration.
- **simply-daemon** — the hub. REST + WebSocket server, session management, MCP registry, storage coordination.
- **simply-rpc** — generic trait-over-network RPC framework. Proc macro auto-generates server dispatch + client impls from annotated traits.

See [CORE_SERVICE.md](../../../designs/CORE_SERVICE.md) for the communication protocol and RPC framework.

---

## Stages

### Stage 1 — Workspace Restructure (complete)

Renamed crates from `noema-*` to `simply-*`, created `simply-daemon` with `DaemonApi` trait, merged `noema-mcp-core` into the daemon.

### Stage 2 — Daemon (complete)

Built the daemon as a working service:

1. **In-process** — `EmbeddedDaemon` implements 7 API traits directly. Noema wired to it, validating the API surface.
2. **Standalone binary** — `simply-daemon` binary with config, signal handling, structured logging.
3. **WebSocket + REST** — Generic WS server (takes dispatch callback), REST for assets + management.
4. **RPC framework** — `simply-rpc` crate with proc macro: `#[rpc_service("prefix")]` auto-generates dispatch + client macros.
5. **RemoteDaemon** — client-side WS + HTTP implementation.

### Stage 3 — REST-First Transport (complete)

All request/response methods use REST. WebSocket is streaming-only (SessionApi). Zero public API change for existing clients.

1. **REST annotations** — `#[rpc(get = "/path/{param}")]`, `#[rpc(post = "/path")]`, `#[rpc(put)]`, `#[rpc(delete)]` on all API traits. `#[rpc(stream = "/path")]` for WebSocket streams.
2. **Route metadata** — `RouteMeta` with `RouteKind::Rest(HttpMethod)` / `RouteKind::Stream`, plus `binary_response`, `binary_upload`, `immutable_cache`, `no_tool` flags.
3. **REST dispatch** — `RestDispatcher` with `matchit` crate for URL routing, `RestResult` carries `RouteMeta` for response encoding.
4. **Axum server** — replaced hand-rolled hyper with axum. REST + admin on single port.
5. **Binary transfer** — `BinaryResponse` (detected from return type) + `BinaryUpload` for native HTTP binary transfer. `immutable_cache` adds Cache-Control + ETag.
6. **Client codegen** — `impl_remote_xxx!` generates reqwest HTTP calls for REST, WS for streams. `RemoteDaemon` uses both transports.
7. **Tool generation** — `DaemonToolService` exposes REST methods as `ToolService` tools. `CompositeToolService` merges multiple tool sources.
8. **Transport in simply-rpc** — `WsConnection<E>`, `NotificationDemux<E>`, `WsServer`, protocol types all live in simply-rpc.
9. **DaemonInfoApi** — `/daemon` (health), `/daemon/kill`, `/daemon/version`.
10. **Admin page** — dashboard calls REST endpoints via `fetch()`.
11. **90 tests** — metadata, dispatch, client round-trip, raw HTTP (reqwest), raw WS (tungstenite), binary transfer.

### Stage 4 — Service Extraction (complete)

Broke EmbeddedDaemon monolith into focused service objects.

1. **McpService** implements `McpApi` + `OAuthApi` directly (no more delegation layer).
2. **Extracted services** — `ModelService`, `AssetService`, `VoiceService`, `DaemonInfoService` each implement their API trait.
3. **EmbeddedDaemon** keeps `SessionApi` + `ConversationApi` (tightly coupled to session state), delegates rest for `DaemonApi` compat.
4. **main.rs** registers individual services with `RestDispatcher` instead of the monolith.

### Stage F — Typed Content Dispatch (complete)

Trait-based content conversion for the RPC → MCP boundary. Types declare how they serialize (output) and deserialize (input) as MCP content.

1. **`IntoContent` trait + `ContentPart` enum** — `BinaryResponse` → binary, everything else → JSON. `#[derive(IntoContent)]` proc macro.
2. **`rest_dispatch_as_content`** — RPC macro generates typed content dispatch. Calls `IntoContent` on concrete return types before type erasure.
3. **`FromContent` trait** — inverse direction. `BinaryUpload` decodes MCP image/audio content (base64 → bytes).
4. **`DaemonToolService`** uses `rest_dispatch_as_content` → `ContentPart` → `ToolResultContent`. No runtime metadata branching.

### Deferred

- **DocumentApi / GDocs sidecar** — deferred to Content phase (sidecar design TBD)
- **Peer registry** — deferred to Lumina phase (needed when multiple clients connect)
- **Voice pipeline** — moved to Voice phase
- **Auth (remote access)** — deferred post-v1. REST binds to localhost only for now.
- **Single-port merge** — WS upgrade in axum handler (currently separate WS server port)
- **RpcSchema** — proper JSON Schema derivation for tool input schemas (currently placeholder)
- **Binary upload query params** — `#[rpc(post = "/asset?mime_type", body = raw)]` syntax
