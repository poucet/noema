# Foundation — Tasks

**Phase:** Foundation
**Status:** Complete
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

### Done

| | What |
|---|------|
| ✅ | Parse REST (`get`, `post`, `put`, `delete`) and stream (`stream`) path annotations |
| ✅ | Generate `RouteMeta` with `RouteKind::Rest(HttpMethod)` / `RouteKind::Stream` |
| ✅ | Generate `rest_dispatch_by_name` — match method name → call trait, deserialize params |
| ✅ | Extract `///` doc comments into metadata for tool descriptions |
| ✅ | Generate `RestService` trait impl (object-safe, matchit-routed via `RestDispatcher`) |
| ✅ | Generate REST client code in `impl_remote_xxx!` — `rest_call` for REST, `rpc_call` for stream |
| ✅ | `BinaryResponse` type (detected from return type) + `immutable_cache` annotation |
| ✅ | Remove obsolete `base64_return` and `rest_get` annotations |
| ✅ | `RestDispatcher` with matchit routing |
| ✅ | Axum HTTP server replaces hand-rolled hyper (daemon + tests) |
| ✅ | `RestResult` carries `RouteMeta` for response encoding (binary, cache headers) |
| ✅ | Move protocol types (`WsRequest`, `WsResponse`, etc.) to simply-rpc |
| ✅ | Rename `ws/` → `net/` in daemon |
| ✅ | REST annotations on all API traits + stream paths on SessionApi |
| ✅ | SessionApi reshaped: seed_context → CreateSessionOptions, reload removed |
| ✅ | `DaemonInfoApi` trait (health, kill, version) |
| ✅ | `RestDispatcher` wired with all services + `DaemonInfoApi` |
| ✅ | `RemoteDaemon` uses reqwest for REST, WS for streams |
| ✅ | `WsConnection.rest_call` multiplexes REST over WebSocket |
| ✅ | `mime_type` naming throughout (not `media_type`) |
| ✅ | 90 tests: metadata, dispatch, client round-trip, raw HTTP (reqwest), raw WS (tungstenite), BinaryResponse |

### Remaining

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.A | ✅ | `BinaryUpload` type — native binary upload via raw HTTP body + Content-Type. Removed `base64_param`. | P0 | M |
| 3.B | ✅ | Move WS transport to simply-rpc — generic `WsConnection<E>` + `WsServer`, daemon uses thin wrapper | P0 | L |
| 3.C | ✅ | Single axum server with `ServerConfig` (REST + WS dispatch ready, WS upgrade pending full merge) | P0 | M |
| 3.D | ✅ | Remove old WS dispatch for REST methods — WS now streaming-only (SessionApi) | P0 | S |
| 3.E | ✅ | `DaemonToolService` — exposes REST methods as tools (schema derivation via RpcSchema deferred) | P0 | M |
| 3.F | ✅ | `CompositeToolService` — wraps multiple `ToolService` impls, agent sees merged tool list | P0 | M |
| 3.G | ✅ | Admin webpage calls REST endpoints — sessions, models, kill via `fetch()` | P1 | M |

---

## Stage 4 — Service Extraction

**Goal:** Break EmbeddedDaemon monolith into focused service objects. Each internal service implements its API trait directly. `main.rs` registers individual services with RestDispatcher. EmbeddedDaemon keeps SessionApi + ConversationApi (tightly coupled) and delegates the rest for `DaemonApi` backward compat.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 4.1 | ✅ | `McpService` implements `McpApi` + `OAuthApi` directly (move inherent methods to trait impls) | P0 | S |
| 4.2 | ✅ | Extract `ModelService`, `AssetService`, `VoiceService`, `DaemonInfoService` | P0 | M |
| 4.3 | ✅ | Refactor `EmbeddedDaemon` — hold `Arc` services, expose accessors, thin delegation for `DaemonApi` | P0 | M |
| 4.4 | ✅ | `main.rs` registers individual services with `RestDispatcher` | P0 | S |
