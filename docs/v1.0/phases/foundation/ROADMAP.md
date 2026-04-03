# Foundation

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 — everything else depends on this.
**Status:** In Progress (Stage 3)
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
3. **WebSocket + REST** — Generic WS server (takes dispatch callback), REST for assets + management (`/health`, `/kill`).
4. **RPC framework** — `simply-rpc` crate with proc macro: `#[rpc_service("prefix")]` auto-generates dispatch + client macros. Supports `#[rpc(stream)]`, `#[rpc(base64_param/return)]`, `#[rpc(rest_get)]`, `#[rpc(skip)]`.
5. **RemoteDaemon** — client-side WS implementation with 7 one-liner macro invocations.

### Stage 3 — REST-First Transport (in progress)

Switch all request/response methods from WebSocket to REST. WebSocket becomes streaming-only. **Zero public API change** — `DaemonApi` traits and `RemoteDaemon` keep the same interface; only the transport changes.

1. **Codegen** — extend `simply-rpc` proc macro to parse REST path annotations (`get = "/path/{param}"`), generate `RestMeta`, `rest_dispatch`, and `ToolDefinition` from doc comments + param schemas.
2. **REST server** — auto-route from `ServiceMeta` metadata, replacing manual route wiring.
3. **Trait annotations** — add REST annotations to all 7 existing API traits. Add `DaemonInfoApi` for `/health`, `/kill`, `/version`.
4. **Tool generation** — REST methods implement `ToolService` trait (in-process, no MCP server), registered in `McpToolRegistry` alongside external tools.
5. **Client codegen** — `impl_remote_xxx!` generates HTTP client code for REST methods, WebSocket code for stream methods. `RemoteDaemon` holds base URL + lazy WS.
6. **Cleanup** — remove WebSocket dispatch for REST-annotated methods, remove hardcoded admin routes.
7. **Admin page** — update dashboard to call REST endpoints via `fetch()`.

### Deferred

- **DocumentApi / GDocs sidecar** — deferred to content phase (sidecar design TBD)
- **Peer registry** — deferred to Lumina phase (needed when multiple clients connect)
- **Voice pipeline** — moved to voice phase
- **Auth (remote access)** — deferred post-v1. REST binds to localhost only for now.
