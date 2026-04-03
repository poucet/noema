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

## Stage 3 — REST-First Transport

**Goal:** Switch all request/response methods from WebSocket to REST. WebSocket becomes streaming-only. Zero public API change — DaemonApi traits stay identical, RemoteDaemon keeps the same interface.

**Design:** [RPC_FRAMEWORK.md](../../../designs/RPC_FRAMEWORK.md)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.1 | ⬜ | Parse REST path annotations in `simply-rpc` proc macro (`get = "/path/{param}"`, `post`, `put`, `delete`) | P0 | M |
| 3.2 | ⬜ | Generate `RestMeta` in `ServiceMeta` (http method, path template, method name) | P0 | S |
| 3.3 | ⬜ | Generate `rest_dispatch` on service struct — match HTTP method + path segments → call trait method, deserialize path/body/query params | P0 | L |
| 3.4 | ⬜ | Extract `///` doc comments into metadata for tool descriptions | P0 | S |
| 3.5 | ⬜ | Add `RpcSchema` trait for JSON Schema overrides; generate `tool_definitions()` on service struct using doc comments + `schemars`/`RpcSchema` | P0 | M |
| 3.6 | ⬜ | Wire REST auto-routing in `simply-daemon` REST server — iterate registered services, match from metadata (replace manual route wiring) | P0 | M |
| 3.7 | ⬜ | Add REST annotations to all 7 existing API traits (ConversationApi, SessionApi, AssetApi, McpApi, OAuthApi, ModelApi, VoiceApi) | P0 | M |
| 3.8 | ⬜ | Add `DaemonInfoApi` trait with `health`, `kill`, `version` — remove hardcoded admin routes | P0 | S |
| 3.9 | ⬜ | Implement `ToolService` from generated `tool_definitions` + `rest_dispatch` — register in `McpToolRegistry` as daemon tools (in-process, no MCP server) | P0 | M |
| 3.10 | ⬜ | Update `impl_remote_xxx!` codegen — generate HTTP client code (reqwest) for REST methods, keep WS for stream methods | P0 | L |
| 3.11 | ⬜ | Update `RemoteDaemon` — hold base URL + lazy WS connection, HTTP for REST, WS only opened for stream methods | P0 | M |
| 3.12 | ⬜ | Remove WebSocket dispatch for REST-annotated methods (WS now streaming-only) | P0 | S |
| 3.13 | ⬜ | Update admin webpage to call REST endpoints via `fetch()` (list sessions, manage MCP, view models) | P1 | M |
