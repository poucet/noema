# Foundation

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 — everything else depends on this.
**Tasks:** [TASKS.md](TASKS.md)

---

## Goal

Restructure the workspace to match the target architecture and build the daemon hub that all clients connect to.

- **simply-core** — library crate, internal to simply-daemon. LLM providers, MCP server/client, agent orchestration. No external crate depends on it.
- **simply-daemon** — the hub. Wires simply-core with UCM storage, WebSocket server (rich clients), REST server (triggers), MCP client (action services), session management, and later event bus / voice pipeline.

See [CORE_SERVICE.md](../../../designs/CORE_SERVICE.md) for the full communication protocol.

---

## Stages

### Stage 1 — Workspace Restructure (complete)

Renamed crates from `noema-*` to `simply-*`, created `simply-daemon` with `DaemonApi` trait, merged `noema-mcp-core` into the daemon. See [HANDOFF.md](HANDOFF.md) for details.

### Stage 2 — Daemon

Build the daemon as a working service. Key milestones:

1. **In-process first** — `EmbeddedDaemon` implements `DaemonApi` directly, wrapping `ConversationManager`. Noema wires to it immediately, validating the API surface before any networking.
2. **Standalone binary** — `simply-daemon` binary with config loading, signal handling, structured logging.
3. **WebSocket + REST** — Remote `DaemonApi` over WebSocket, REST endpoints for triggers and health.
4. **Registry + MCP client** — Peer tracking, global tool registry, connecting to action services.
5. **Storage migration** — Move storage from `simply-core` to `simply-daemon` once wiring is validated.

---

## Dependencies

```
Stage 1 → Stage 2 (sequential)
```
