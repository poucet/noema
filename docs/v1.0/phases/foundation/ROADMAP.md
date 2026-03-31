# Foundation

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 — everything else depends on this.
**Complexity:** L

---

## Goal

Restructure the workspace to match the target architecture: separate `simply-core` (library: LLM + MCP + agent) from `simply-daemon` (the hub: storage + WebSocket/REST/MCP + events). All crate renames and structural moves happen here so that parallel work streams don't collide with moving code.

Python Lumina remains operational throughout — no big bang cutover.

### Key architectural distinction

- **simply-core** — library crate, internal to simply-daemon. LLM providers, MCP server/client, agent orchestration. No external crate depends on it.
- **simply-daemon** — the hub. Wires simply-core with UCM storage, WebSocket server (rich clients), REST server (triggers), MCP client (action services), session management, and later event bus / voice pipeline.

See [CORE_SERVICE.md](../../../designs/CORE_SERVICE.md) for the full communication protocol.

---

## Stages

### Stage 1 — Workspace Restructure

**Goal:** Rename crates and separate core library from daemon per [ARCHITECTURE.md](../../../designs/ARCHITECTURE.md).

**Complexity:** M

| Current | Target | Change |
|---------|--------|--------|
| `noema-core/` | `simply-core/` | Rename — becomes the library crate (agent, MCP, LLM) |
| `noema-core/llm/` | `simply-core/llm/` | Stays as sub-crate of core (no extraction) |
| `noema-core/src/storage/` | `simply-daemon/src/storage/` | Move to daemon crate |
| `noema-audio/` | `simply-audio/` | Rename |
| `noema-mcp-core/` | merge into `simply-daemon/src/mcp/` | Merge — daemon concern (storage, sessions, agent orchestration) |
| *(new)* | `simply-daemon/` | New crate — daemon binary |
| `noema-desktop/` | `noema-desktop/` | No change |
| `noema-ext/` | `noema-ext/` | No change |
| `noema-mcp-gdocs/` | — | Deferred (Google integration is post-v1) |

**Tasks:**
- [ ] Rename `noema-core/` → `simply-core/`, update `Cargo.toml` package name + all workspace references
- [ ] Move storage code out of `simply-core` → `simply-daemon/src/storage/`
- [ ] Rename `noema-audio/` → `simply-audio/`, update references
- [ ] Merge `noema-mcp-core/` into `simply-daemon/src/mcp/`, remove standalone crate
- [ ] Create `simply-daemon/` crate (initially just re-exports, wires core + storage)
- [ ] Update workspace `Cargo.toml` members list
- [ ] Verify `noema-desktop` builds with restructured dependencies

**Verify:** `cargo check --workspace` passes. Noema desktop still launches.

---

### Stage 2 — Daemon

**Goal:** `simply-daemon` runs as a daemon. Noema connects as a client via WebSocket.

**Complexity:** L

**Tasks:**
- [ ] `simply-daemon` binary: startup, config loading, graceful shutdown
- [ ] WebSocket server: session management, context seeding, message send/receive (JSON, Rust types)
- [ ] REST server: `/events` (trigger inbound), `/register` (service registration), `/health`
- [ ] Session manager: in-memory conversation state, ephemeral + persistent (UCM-backed) modes
- [ ] Peer registry: track connected clients and services, global MCP tool registry
- [ ] MCP client: connect to registered action services, discover tools
- [ ] Noema's React frontend connects to daemon directly via WebSocket for chat
- [ ] Noema's Tauri backend handles OS-level only (slash commands, file access)
- [ ] UCM storage owned by the daemon — single writer, no SQLite contention
- [ ] Daemon lifecycle: decide startup approach (standalone daemon vs. first-client-spawns)

**Verify:**
- Start simply-daemon, start Noema.
- Chat works through WebSocket.

---

## Dependencies

```
Stage 1 → Stage 2 (sequential)
```
