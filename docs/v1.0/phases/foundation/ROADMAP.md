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
- **simply-daemon** — the hub. WebSocket + REST server, session management, MCP registry, storage coordination.
- **simply-rpc** — generic trait-over-network RPC framework. Proc macro auto-generates server dispatch + client impls from annotated traits.

See [RPC_FRAMEWORK.md](../../../designs/RPC_FRAMEWORK.md) for the RPC design.
See [CORE_SERVICE.md](../../../designs/CORE_SERVICE.md) for the communication protocol.

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

### Deferred

- **DocumentApi / GDocs sidecar** — deferred to content phase (sidecar design TBD)
- **Peer registry** — deferred to Lumina phase (needed when multiple clients connect)
- **Voice pipeline** — moved to voice phase
