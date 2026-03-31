# Foundation

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 — everything else depends on this.
**Complexity:** L

---

## Goal

Restructure the workspace to match the target architecture and extract the core daemon service. All crate renames and structural moves happen here so that parallel work streams don't collide with moving code.

Python Lumina remains operational throughout — no big bang cutover.

---

## Stages

### Stage 1 — Workspace Restructure

**Goal:** Rename crates to match the [ARCHITECTURE.md](../../../designs/ARCHITECTURE.md) workspace structure.

**Complexity:** S

| Current | Target | Change |
|---------|--------|--------|
| `noema-core/` | `simply-core/` | Rename |
| `noema-core/llm/` | `simply-llm/` | Extract to top-level crate + rename |
| `noema-audio/` | `simply-audio/` | Rename |
| `noema-mcp-core/` | merge into `simply-core/src/mcp/` | Merge |
| `noema-desktop/` | `noema-desktop/` | No change |
| `noema-ext/` | `noema-ext/` | No change |
| `noema-ui/` | `noema-ui/` | No change (frontend) |
| `noema-mcp-gdocs/` | — | Deferred (Google integration is post-v1) |
| `commands/` | `commands/` | No change |
| `config/` | `config/` | No change |

**Tasks:**
- [ ] Rename `noema-core/` → `simply-core/`, update `Cargo.toml` package name + all workspace references
- [ ] Extract `noema-core/llm/` → top-level `simply-llm/`, update dependencies
- [ ] Rename `noema-audio/` → `simply-audio/`, update references
- [ ] Merge `noema-mcp-core/` into `simply-core/src/mcp/`, remove standalone crate
- [ ] Update workspace `Cargo.toml` members list
- [ ] Verify `noema-desktop` builds with renamed dependencies

**Verify:** `cargo check --workspace` passes. Noema desktop still launches.

---

### Stage 2 — Core Service Extraction

**Goal:** `simply-core` runs as a daemon. Noema connects as a client.

**Complexity:** L

**Tasks:**
- [ ] Extract shared logic from in-process usage into `simply-core` service binary
- [ ] gRPC server (tonic) with initial RPCs: `prompt`, `run_turn`, `list_models`
- [ ] Noema's Tauri backend becomes a core client (refactor from direct embedding)
- [ ] UCM storage owned by the service — single writer, no SQLite contention
- [ ] Service lifecycle: decide startup approach (standalone daemon vs. first-client-spawns)

**Verify:**
- Start core service, start Noema.
- Chat works through the service.

---

## Dependencies

```
Stage 1 → Stage 2 (sequential)
```
