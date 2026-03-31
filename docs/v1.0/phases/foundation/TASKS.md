# Foundation — Tasks

**Phase:** Foundation
**Status:** In Progress
**Roadmap:** [ROADMAP.md](ROADMAP.md)

---

## Stage 1 — Workspace Restructure

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 1.1 | ✅ | Rename `noema-core/` → `simply-core/`, update `Cargo.toml` package name + all workspace refs | P0 | S |
| 1.2 | ✅ | Rename `noema-audio/` → `simply-audio/`, update references | P0 | S |
| 1.3 | ✅ | Create `simply-daemon/` crate with `DaemonApi` trait | P0 | S |
| 1.4 | ✅ | Merge `noema-mcp-core/` into `simply-daemon/src/mcp/`, remove standalone crate | P0 | M |
| 1.5 | ✅ | Update workspace `Cargo.toml` members list | P0 | S |
| 1.6 | ⏳ | Verify `noema-desktop` builds with restructured deps | P0 | S |

### Task Details

**1.1 — Rename noema-core → simply-core**
- Rename directory `noema-core/` → `simply-core/`
- Update `Cargo.toml` package name to `simply-core`
- Update `noema-core/llm/` → `simply-core/llm/`, package name to `simply-llm`
- Update all `use noema_core::` → `use simply_core::` across workspace
- Update all `path = "../noema-core"` dependency paths

**1.2 — Rename noema-audio → simply-audio**
- Rename directory `noema-audio/` → `simply-audio/`
- Update `Cargo.toml` package name to `simply-audio`
- Update workspace references and dependent crates

**1.3 — Create simply-daemon crate**
- Library + binary crate at `simply-daemon/`
- `lib.rs` exposes a trait-based `DaemonApi` — the clean Rust interface
- `main.rs` is the standalone daemon runner (one way to host it)
- Depends on `simply-core` for LLM, MCP protocol, agent types
- Initially a skeleton that compiles
- **Key pattern:** Noema/Lumina depend on the trait, not on how it's hosted.
  Two implementations planned:
  - **In-process** — daemon linked directly into the binary (for testing, single-binary deploys)
  - **Remote** — calls go over WebSocket to a separate daemon process
  This lets Noema work against the daemon API immediately, before WebSocket is built.

**1.4 — Merge noema-mcp-core into simply-daemon**
- Move `noema-mcp-core/src/tools.rs` → `simply-daemon/src/mcp/tools.rs`
- This code depends on StorageCoordinator, Session, agent orchestration — daemon concerns
- Remove `noema-mcp-core/` crate from workspace
- Update imports to use daemon-local paths

**1.5 — Update workspace Cargo.toml**
- Remove old member paths (`noema-core`, `noema-audio`, `noema-mcp-core`)
- Add new member paths (`simply-core`, `simply-audio`, `simply-daemon`)
- Verify workspace resolver and default-members

**1.6 — Verify noema-desktop builds**
- Update `noema-desktop/src-tauri/Cargo.toml` deps to point to renamed crates
- Ensure `cargo check --workspace` passes
- Confirm desktop app still launches against new structure

---

## Stage 2 — Daemon

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ⬜ | `DaemonApi` trait: define the core API surface | P0 | M |
| 2.2 | ⬜ | In-process implementation of `DaemonApi` | P0 | M |
| 2.3 | ⬜ | Wire Noema desktop to use in-process daemon | P0 | L |
| 2.4 | ⬜ | Session manager: in-memory state, ephemeral + persistent modes | P0 | M |
| 2.5 | ⬜ | Daemon binary: startup, config loading, graceful shutdown | P0 | M |
| 2.6 | ⬜ | WebSocket server + remote `DaemonApi` implementation | P0 | L |
| 2.7 | ⬜ | REST server: `/events`, `/register`, `/health` endpoints | P0 | M |
| 2.8 | ⬜ | Peer registry: connected clients, global MCP tool registry | P0 | M |
| 2.9 | ⬜ | MCP client: connect to action services, discover tools | P1 | M |
| 2.10 | ⬜ | Move storage from `simply-core` → `simply-daemon` | P0 | M |

### Task Details

**2.1 — DaemonApi trait**
- Define the trait in `simply-daemon/src/api.rs`
- Core operations: create_session, send_message, list_sessions, register_mcp, etc.
- Async trait — both in-process and remote impls are async
- Message/event types as plain Rust structs (serde-serializable for future WebSocket use)

**2.2 — In-process implementation**
- `simply-daemon/src/embedded.rs` — implements `DaemonApi` directly
- Wires simply-core agent, MCP registry, storage coordinator in-process
- No networking — pure Rust calls
- This is the first way Noema/Lumina can use the daemon

**2.3 — Wire Noema desktop to in-process daemon**
- Replace Noema's direct simply-core usage with `DaemonApi` calls
- Use the in-process implementation — same binary, no separate process
- Validates the API surface is complete before building WebSocket layer

**2.4 — Session manager**
- `simply-daemon/src/session/` module
- In-memory conversation state
- Ephemeral mode (no persistence) and persistent mode (UCM-backed)
- Context seeding for new sessions

**2.5 — Daemon binary**
- `simply-daemon/src/main.rs` with tokio runtime
- Config loading from `~/.config/simply/` or env vars
- Signal handling (SIGTERM, SIGINT) for graceful shutdown
- Structured logging with tracing

**2.6 — WebSocket server + remote DaemonApi**
- `simply-daemon/src/ws/` — WebSocket server
- `simply-daemon/src/remote.rs` — client-side `DaemonApi` impl over WebSocket
- JSON message protocol per [CORE_SERVICE.md](../../../designs/CORE_SERVICE.md)
- Noema can swap in-process for remote without code changes

**2.7 — REST server**
- `simply-daemon/src/rest/` module
- `POST /events` — trigger inbound events
- `POST /register` — service registration
- `GET /health` — health check

**2.8 — Peer registry**
- `simply-daemon/src/registry/` module
- Track connected WebSocket clients and registered services
- Global MCP tool registry (aggregate tools from all connected action services)

**2.9 — MCP client**
- `simply-daemon/src/mcp/client.rs`
- Connect to registered action services as MCP client
- Discover and cache available tools
- Forward tool calls from agent to appropriate service

**2.10 — Move storage from simply-core → simply-daemon**
- Move `simply-core/src/storage/` → `simply-daemon/src/storage/`
- Update `simply-core` to remove storage module
- Re-export or adjust imports in `simply-daemon`
- Storage includes: coordinator, session, traits, document_resolver, ids
- Deferred from Stage 1 — easier once daemon is wired up and verifiable

---

## Dependencies

```
Stage 1:
  1.1 (rename core) ─┐
  1.2 (rename audio) ─┤→ 1.5 (update workspace) → 1.6 (verify desktop)
  1.3 (create daemon) ─┤
  1.4 (merge mcp-core) ┘

Stage 2:
  2.1 (trait) → 2.2 (in-process impl) → 2.3 (wire Noema)
  2.4 (sessions) feeds into 2.2
  2.5 (binary) → 2.6 (WebSocket + remote impl) → 2.7 (REST) → 2.8 (registry)
  2.9 (MCP client) after 2.8
  2.10 (move storage) after 2.3 is validated
```
