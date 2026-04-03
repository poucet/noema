# Foundation — Tasks

**Phase:** Foundation
**Status:** In Progress (Stage 3)
**Roadmap:** [ROADMAP.md](ROADMAP.md)

---

## Stage 1 — Workspace Restructure (Complete)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 1.1 | ✅ | Rename `noema-core/` → `simply-core/`, update `Cargo.toml` package name + all workspace refs | P0 | S |
| 1.2 | ✅ | Rename `noema-audio/` → `simply-audio/`, update references | P0 | S |
| 1.3 | ✅ | Create `simply-daemon/` crate with `DaemonApi` trait | P0 | S |
| 1.4 | ✅ | Merge `noema-mcp-core/` into `simply-daemon/src/mcp/`, remove standalone crate | P0 | M |
| 1.5 | ✅ | Update workspace `Cargo.toml` members list | P0 | S |
| 1.6 | ✅ | Verify `noema-desktop` builds with restructured deps | P0 | S |

---

## Stage 2 — Daemon (Complete)

**Goal:** All logic in the daemon so Lumina can be built on top of the same API.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ✅ | `DaemonApi` trait: define the core API surface | P0 | M |
| 2.2 | ✅ | In-process implementation of `DaemonApi` | P0 | M |
| 2.3 | ✅ | Wire Noema desktop to use in-process daemon | P0 | L |
| 2.3.1 | ✅ | Decouple Noema from simply-core/llm — only use daemon traits; rename `noema-desktop` → `noema` | P0 | L |
| 2.3.2 | ✅ | Move MCP commands + OAuth flow into daemon (McpApi + OAuthApi) | P0 | M |
| 2.4 | ✅ | Stable OAuth callback port on daemon | P0 | S |
| 2.6 | ✅ | Daemon binary: startup, config loading, graceful shutdown | P0 | M |
| 2.7 | ✅ | WebSocket server + remote `DaemonApi` implementation | P0 | L |
| 2.7.1 | ✅ | Smart discovery: `connect_or_host()`, Noema uses `Arc<dyn DaemonApi>` | P0 | M |
| 2.7.2 | ✅ | `simply-rpc` proc macro: auto-generate WS server dispatch + client impls | P0 | M |
| 2.8 | ✅ | REST server for asset serving (`GET /asset/{hash}`) + management (`/health`, `/kill`) | P1 | S |

### Deferred to later phases

| # | | Task | Deferred to | Reason |
|---|---|------|-------------|--------|
| 2.5 | ⏸️ | DocumentApi on daemon | Content phase | Blocked on sidecar design; not needed for Lumina |
| 2.5.1 | ⏸️ | Rewrite Noema gdocs.rs | Content phase | Depends on 2.5 |
| 2.9 | ⏸️ | Peer registry | Lumina phase | Needed when multiple clients connect |
| 2.10 | ⏸️ | MCP client for action services | Content phase | Needed for sidecar pattern |

---

## Stage 3 — REST-First Transport + Single Port

**Goal:** Single port serves REST and WebSocket. All request/response methods are REST. Sessions are per-path WebSocket connections. Zero public API change for existing clients.

**Design:** [CORE_SERVICE.md](../../../designs/CORE_SERVICE.md)

### Codegen (simply-rpc)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.1 | ✅ | Parse REST path annotations (`get = "/path/{param}"`, `post`, `put`, `delete`) | P0 | M |
| 3.2 | ✅ | Parse stream path annotations (`stream = "/path"`) — WebSocket upgrade routes | P0 | S |
| 3.3 | ✅ | Generate `RouteMeta` in `ServiceMeta` with `RouteKind::Rest(HttpMethod)` or `RouteKind::Stream` | P0 | S |
| 3.4 | ✅ | Generate `rest_dispatch_by_name` — match method name → call trait method, deserialize params | P0 | L |
| 3.5 | ✅ | Extract `///` doc comments into metadata for tool descriptions | P0 | S |
| 3.6 | ✅ | Generate `RestService` trait impl (object-safe dispatch by method name) | P0 | S |
| 3.7 | ✅ | Generate REST client code in `impl_remote_xxx!` — `rest_call` for REST methods, `rpc_call` for stream | P0 | L |
| 3.8 | ✅ | `BinaryResponse` type — detected from return type, `immutable_cache` annotation | P0 | M |
| 3.9 | ✅ | Remove obsolete `base64_return` and `rest_get` annotations | P0 | S |
| 3.10 | ⬜ | `RpcSchema` trait for JSON Schema overrides; generate `tool_definitions()` | P0 | M |
| 3.11a | ⬜ | Native binary upload/download — raw HTTP body instead of base64 encoding. Upload: `POST /path?metadata=...` with raw bytes. Download: `BinaryResponse` (done). Remove `base64_param`. | P0 | M |

### Server + transport

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.11 | ✅ | `RestDispatcher` with matchit routing (literal-before-param priority) | P0 | M |
| 3.12 | ✅ | Axum HTTP server replaces hand-rolled hyper (daemon + tests) | P0 | M |
| 3.13 | ✅ | `RestResult` carries `RouteMeta` for response encoding (binary, cache headers) | P0 | S |
| 3.14 | ✅ | Move generic protocol types (`WsRequest`, `WsResponse`, etc.) to simply-rpc | P0 | S |
| 3.15 | ✅ | Rename `ws/` → `net/` in daemon | P0 | S |
| 3.16 | ⬜ | Move WS transport (`WsConnection`, `server.rs`) to simply-rpc (generic over Stream type) | P0 | L |
| 3.17 | ⬜ | Merge REST + WS into single axum port (WS upgrade on stream paths) | P0 | M |

### Daemon wiring

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.18 | ✅ | Add REST annotations to all API traits | P0 | M |
| 3.19 | ✅ | Add stream path annotations to SessionApi (`/session/new`, `/session/{id}`) | P0 | S |
| 3.20 | ✅ | Reshape SessionApi: fold `seed_context` into `CreateSessionOptions`, remove `reload` | P0 | S |
| 3.21 | ✅ | Add `DaemonInfoApi` trait (`health`, `kill`, `version`) | P0 | S |
| 3.22 | ✅ | Wire `RestDispatcher` with all services in daemon + `DaemonInfoApi` | P0 | M |
| 3.23 | ✅ | `RemoteDaemon` uses reqwest for REST via `rest_call`, WS for streams | P0 | M |
| 3.24 | ✅ | `WsConnection.rest_call` multiplexes REST over WebSocket | P0 | S |
| 3.25 | ⬜ | Implement `ToolService` from generated tools — register in `McpToolRegistry` | P0 | M |
| 3.26 | ⬜ | Remove old WS dispatch for REST-annotated methods | P0 | S |
| 3.27 | ⬜ | Update admin webpage to call REST endpoints via `fetch()` | P1 | M |

### Tests (90 total)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.T1 | ✅ | REST metadata tests (paths, HTTP methods, doc comments, no_tool, binary_response, immutable_cache) | P0 | S |
| 3.T2 | ✅ | RestDispatcher unit tests (GET/POST/PUT/DELETE, path params, body params, matchit routing) | P0 | M |
| 3.T3 | ✅ | Client macro round-trip tests (REST `rest_call` + WS `rpc_call`) | P0 | S |
| 3.T4 | ✅ | Raw HTTP integration tests — reqwest against axum server, full CRUD | P0 | M |
| 3.T5 | ✅ | Stream path metadata + REST/WS coexistence tests | P0 | S |
| 3.T6 | ✅ | Raw WebSocket integration test (tokio-tungstenite) | P0 | M |
| 3.T7 | ✅ | BinaryResponse raw HTTP tests — store, get raw bytes, caching headers, round-trip | P0 | M |
