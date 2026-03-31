# Foundation — Tasks

**Phase:** Foundation
**Status:** In Progress
**Roadmap:** [ROADMAP.md](ROADMAP.md)

---

## Stage 1 — Workspace Restructure

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 1.1 | ✅ | Rename `noema-core/` → `simply-core/`, update `Cargo.toml` package name + all workspace refs | P0 | S |
| 1.2 | ⬜ | Move storage code out of `simply-core/` → `simply-daemon/src/storage/` | P0 | M |
| 1.3 | ⬜ | Rename `noema-audio/` → `simply-audio/`, update references | P0 | S |
| 1.4 | ⬜ | Merge `noema-mcp-core/` into `simply-daemon/src/mcp/`, remove standalone crate | P0 | M |
| 1.5 | ⬜ | Create `simply-daemon/` crate (binary, wires core + storage) | P0 | S |
| 1.6 | ⬜ | Update workspace `Cargo.toml` members list | P0 | S |
| 1.7 | ⬜ | Verify `noema-desktop` builds with restructured deps | P0 | S |

### Task Details

**1.1 — Rename noema-core → simply-core**
- Rename directory `noema-core/` → `simply-core/`
- Update `Cargo.toml` package name to `simply-core`
- Update `noema-core/llm/` → `simply-core/llm/`, package name to `simply-llm`
- Update all `use noema_core::` → `use simply_core::` across workspace
- Update all `path = "../noema-core"` dependency paths

**1.2 — Move storage to daemon**
- Move `simply-core/src/storage/` → `simply-daemon/src/storage/`
- Update `simply-core` to remove storage module
- Re-export or adjust imports in `simply-daemon`
- Storage includes: coordinator, session, traits, document_resolver, ids

**1.3 — Rename noema-audio → simply-audio**
- Rename directory `noema-audio/` → `simply-audio/`
- Update `Cargo.toml` package name to `simply-audio`
- Update workspace references and dependent crates

**1.4 — Merge noema-mcp-core into simply-daemon**
- Move `noema-mcp-core/src/tools.rs` → `simply-daemon/src/mcp/tools.rs`
- This code depends on StorageCoordinator, Session, agent orchestration — daemon concerns
- Remove `noema-mcp-core/` crate from workspace
- Update imports to use daemon-local paths

**1.5 — Create simply-daemon crate**
- New binary crate at `simply-daemon/`
- Depends on `simply-core` for LLM, MCP protocol, agent types
- Owns storage, MCP server, session management
- Initially a skeleton that compiles

**1.6 — Update workspace Cargo.toml**
- Remove old member paths (`noema-core`, `noema-audio`, `noema-mcp-core`)
- Add new member paths (`simply-core`, `simply-audio`, `simply-daemon`)
- Verify workspace resolver and default-members

**1.7 — Verify noema-desktop builds**
- Update `noema-desktop/src-tauri/Cargo.toml` deps to point to renamed crates
- Ensure `cargo check --workspace` passes
- Confirm desktop app still launches against new structure

---

## Stage 2 — Daemon

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ⬜ | Daemon binary: startup, config loading, graceful shutdown | P0 | M |
| 2.2 | ⬜ | WebSocket server: session management, JSON message types | P0 | L |
| 2.3 | ⬜ | REST server: `/events`, `/register`, `/health` endpoints | P0 | M |
| 2.4 | ⬜ | Session manager: in-memory state, ephemeral + persistent modes | P0 | M |
| 2.5 | ⬜ | Peer registry: connected clients, global MCP tool registry | P0 | M |
| 2.6 | ⬜ | MCP client: connect to action services, discover tools | P1 | M |
| 2.7 | ⬜ | UCM storage ownership: single-writer, no SQLite contention | P0 | M |
| 2.8 | ⬜ | Noema frontend: connect to daemon via WebSocket for chat | P0 | L |
| 2.9 | ⬜ | Daemon lifecycle: startup approach decision + implementation | P0 | S |

### Task Details

**2.1 — Daemon binary**
- `simply-daemon/src/main.rs` with tokio runtime
- Config loading from `~/.config/simply/` or env vars
- Signal handling (SIGTERM, SIGINT) for graceful shutdown
- Structured logging with tracing

**2.2 — WebSocket server**
- `simply-daemon/src/ws/` module
- JSON message protocol per [CORE_SERVICE.md](../../../designs/CORE_SERVICE.md)
- Message types: CreateSession, SendMessage, AgentResponse, ToolCall, SessionState
- Per-connection session binding

**2.3 — REST server**
- `simply-daemon/src/rest/` module
- `POST /events` — trigger inbound events
- `POST /register` — service registration
- `GET /health` — health check
- Axum or similar framework

**2.4 — Session manager**
- `simply-daemon/src/session/` module
- In-memory conversation state
- Ephemeral mode (no persistence) and persistent mode (UCM-backed)
- Context seeding for new sessions

**2.5 — Peer registry**
- `simply-daemon/src/registry/` module
- Track connected WebSocket clients
- Track registered services (REST + MCP)
- Global MCP tool registry (aggregate tools from all connected action services)

**2.6 — MCP client**
- `simply-daemon/src/mcp/client.rs`
- Connect to registered action services as MCP client
- Discover and cache available tools
- Forward tool calls from agent to appropriate service

**2.7 — UCM storage ownership**
- Single writer pattern — daemon owns the SQLite connection
- No external crates directly access storage
- Expose storage operations via WebSocket/internal API

**2.8 — Noema frontend reconnect**
- Update `noema-ui/` to connect to daemon WebSocket instead of in-process calls
- Update `noema-desktop/src-tauri/` to only handle OS-level concerns
- Chat flows through daemon, not directly through Tauri backend

**2.9 — Daemon lifecycle**
- Decide: standalone daemon (systemd/launchd) vs. first-client-spawns
- Implement chosen approach
- Document startup/shutdown behavior

---

## Dependencies

```
1.5 (create daemon) → 1.2 (move storage) → 1.4 (merge mcp-core)
1.1 (rename core) ─┐
1.3 (rename audio) ─┤→ 1.6 (update workspace) → 1.7 (verify desktop)
1.5 (create daemon) ─┘

Stage 1 → Stage 2 (sequential)
```
