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
| 3.3 | ✅ | Generate `RestMeta` in `ServiceMeta` (http method, path template, description, no_tool) | P0 | S |
| 3.4 | ✅ | Generate `rest_dispatch` on service struct — match HTTP method + path → call trait method | P0 | L |
| 3.5 | ✅ | Extract `///` doc comments into metadata for tool descriptions | P0 | S |
| 3.6 | ✅ | Generate `RestService` trait impl for each service (object-safe REST dispatch) | P0 | S |
| 3.7 | ⬜ | Add `RpcSchema` trait for JSON Schema overrides; generate `tool_definitions()` | P0 | M |
| 3.8 | ⬜ | Update `impl_remote_xxx!` — generate HTTP client code (reqwest) for REST, WS for stream | P0 | L |

### Daemon wiring

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.9 | ✅ | Add REST annotations to all 7 existing API traits | P0 | M |
| 3.10 | ✅ | Add stream path annotations to SessionApi (`/session/new`, `/session/{id}`, etc.) | P0 | S |
| 3.11 | ✅ | Reshape SessionApi: fold `seed_context` into `CreateSessionOptions`, remove `reload` | P0 | S |
| 3.12 | ✅ | Add `DaemonInfoApi` trait (`health`, `kill`, `version`) with REST annotations | P0 | S |
| 3.13 | ✅ | Wire `RestDispatcher` in daemon REST server — auto-route all services | P0 | M |
| 3.14 | ⬜ | Implement `ToolService` from generated tools — register in `McpToolRegistry` | P0 | M |
| 3.15 | ⬜ | Update `RemoteDaemon` — hold base URL + lazy WS, HTTP for REST, WS for streams | P0 | M |
| 3.16 | ⬜ | Merge REST + WS into single port (WS upgrade detection on stream paths) | P0 | M |
| 3.17 | ⬜ | Remove old WS dispatch for REST-annotated methods | P0 | S |
| 3.18 | ⬜ | Update admin webpage to call REST endpoints via `fetch()` | P1 | M |

### Tests

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.T1 | ✅ | REST metadata tests (paths, HTTP methods, doc comments, no_tool) | P0 | S |
| 3.T2 | ✅ | Direct `rest_dispatch` unit tests (GET/POST/PUT/DELETE, path params, body params) | P0 | M |
| 3.T3 | ✅ | Client macro round-trip tests (WS dispatch) | P0 | S |
| 3.T4 | ✅ | Raw HTTP integration tests (reqwest against real server, full CRUD) | P0 | M |
| 3.T5 | ✅ | Stream path metadata tests | P0 | S |
| 3.T6 | ✅ | REST + WS coexistence tests (REST dispatch on streaming trait, stream paths excluded) | P0 | S |
| 3.T7 | ✅ | Raw WebSocket integration test (tokio-tungstenite client, real server) | P0 | M |
