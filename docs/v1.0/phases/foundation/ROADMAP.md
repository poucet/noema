# Foundation

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 — everything else depends on this.
**Complexity:** L

---

## Goal

Restructure the workspace to match the target architecture: separate `simply-core` (library: LLM + MCP + agent) from `simply-service` (daemon: storage + gRPC + events). All crate renames and structural moves happen here so that parallel work streams don't collide with moving code.

Python Lumina remains operational throughout — no big bang cutover.

### Key architectural distinction

- **simply-core** — platform-agnostic library crate. LLM providers, MCP server/client, agent orchestration with a trait-based `ExecutionContext`. Knows nothing about storage backends or transport protocols.
- **simply-service** — the daemon. Wires simply-core with UCM storage (providing a UCM-backed `ExecutionContext`), gRPC API, and later event bus / voice pipeline. Noema connects as a client.

---

## Stages

### Stage 1 — Workspace Restructure

**Goal:** Rename crates and separate core library from service daemon per [ARCHITECTURE.md](../../../designs/ARCHITECTURE.md).

**Complexity:** M

| Current | Target | Change |
|---------|--------|--------|
| `noema-core/` | `simply-core/` | Rename — becomes the library crate (agent, MCP, LLM) |
| `noema-core/llm/` | `simply-core/llm/` | Stays as sub-crate of core (no extraction) |
| `noema-core/src/storage/` | `simply-service/src/storage/` | Move to service crate |
| `noema-audio/` | `simply-audio/` | Rename |
| `noema-mcp-core/` | merge into `simply-core/src/mcp/` | Merge |
| *(new)* | `simply-service/` | New crate — daemon binary |
| `noema-desktop/` | `noema-desktop/` | No change |
| `noema-ext/` | `noema-ext/` | No change |
| `noema-ui/` | `noema-ui/` | No change (frontend) |
| `noema-mcp-gdocs/` | — | Deferred (Google integration is post-v1) |

**Tasks:**
- [ ] Rename `noema-core/` → `simply-core/`, update `Cargo.toml` package name + all workspace references
- [ ] Extract `ExecutionContext` as a trait in `simply-core` (currently a concrete struct tied to storage IDs)
- [ ] Move storage code out of `simply-core` → `simply-service/src/storage/`
- [ ] Rename `noema-audio/` → `simply-audio/`, update references
- [ ] Merge `noema-mcp-core/` into `simply-core/src/mcp/`, remove standalone crate
- [ ] Create `simply-service/` crate (initially just re-exports, wires core + storage)
- [ ] Update workspace `Cargo.toml` members list
- [ ] Verify `noema-desktop` builds with restructured dependencies

**Verify:** `cargo check --workspace` passes. Noema desktop still launches.

---

### Stage 2 — Service Daemon

**Goal:** `simply-service` runs as a daemon with gRPC API. Noema connects as a client.

**Complexity:** L

**Tasks:**
- [ ] `simply-service` binary: startup, config loading, graceful shutdown
- [ ] UCM-backed `ExecutionContext` implementation in simply-service
- [ ] gRPC server (tonic) with initial RPCs: `prompt`, `run_turn`, `list_models`
- [ ] Noema's Tauri backend becomes a service client (refactor from direct embedding)
- [ ] UCM storage owned by the service — single writer, no SQLite contention
- [ ] Service lifecycle: decide startup approach (standalone daemon vs. first-client-spawns)

**Verify:**
- Start simply-service, start Noema.
- Chat works through the service.

---

## Dependencies

```
Stage 1 → Stage 2 (sequential)
```
