# Foundation — Tasks

**Phase:** Foundation
**Status:** In Progress
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

## Stage 2 — Daemon

**Goal:** All logic in the daemon so Lumina can be built on top of the same API.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ✅ | `DaemonApi` trait: define the core API surface | P0 | M |
| 2.2 | ✅ | In-process implementation of `DaemonApi` | P0 | M |
| 2.3 | ✅ | Wire Noema desktop to use in-process daemon | P0 | L |
| 2.3.1 | ✅ | Decouple Noema from simply-core/llm — only use daemon traits; rename `noema-desktop` → `noema` | P0 | L |
| 2.3.2 | ✅ | Move MCP commands + OAuth flow into daemon (McpApi + OAuthApi) | P0 | M |
| 2.4 | ✅ | Stable OAuth callback port on daemon | P0 | S |
| 2.5 | ⏸️ | DocumentApi on daemon — store/index/query documents (blocked on sidecar design) | P0 | M |
| 2.5.1 | ⏸️ | Rewrite Noema gdocs.rs as thin wrappers (blocked, gdocs disabled temporarily) | P0 | M |
| 2.6 | ✅ | Daemon binary: startup, config loading, graceful shutdown | P0 | M |
| 2.7 | ✅ | WebSocket server + remote `DaemonApi` implementation | P0 | L |
| 2.7.1 | ✅ | Smart discovery: `connect_or_host()`, Noema uses `Arc<dyn DaemonApi>` | P0 | M |
| 2.8 | ⬜ | REST server: `/events`, `/register`, `/health` endpoints | P1 | M |
| 2.9 | ⬜ | Peer registry: connected clients, global MCP tool registry | P1 | M |
| 2.10 | ⬜ | MCP client: connect to action services, discover tools | P2 | M |

### Task Details

**2.4 — Stable OAuth callback port**
- Currently `OAuthService` spins up a temporary callback server on a random port per flow
- Daemon should start a single long-lived callback server on a configured port at startup
- Port comes from config (`~/.config/simply/config.toml` → `oauth_callback_port`)
- Enables predictable redirect URIs for Google OAuth console, cloud Lumina, etc.
- Refactor `OAuthService` to accept a shared callback server rather than creating per-flow

**2.5 — DocumentApi on daemon**
- New trait in `api/document.rs`: `import_document`, `list_documents`, `get_document`, `delete_document`, `sync_document`, `get_document_content`
- Daemon implementation uses `DocumentStore`/`StorageCoordinator` for persistence
- Google-specific fetching stays in `noema-mcp-gdocs` crate (pure Google API client)
- Daemon calls `GoogleDocsClient` to fetch, then stores via its own storage
- This separation means Lumina can import docs without any Google-specific code in the daemon trait

**2.5.1 — Rewrite Noema gdocs.rs**
- Noema gdocs commands become thin wrappers:
  - Google API calls → `noema-mcp-gdocs::GoogleDocsClient` (fetching, listing)
  - Storage/indexing → daemon `DocumentApi` (import, store, query)
- Remove `stores()`/`coordinator()` escape hatches from `EmbeddedDaemon`

**2.6 — Daemon binary**
- `simply-daemon/src/main.rs` with tokio runtime
- Config loading from `~/.config/simply/` or env vars
- Signal handling (SIGTERM, SIGINT) for graceful shutdown
- Structured logging with tracing
- Starts OAuth callback server, MCP server, auto-connect

**2.7 — WebSocket server + remote DaemonApi**
- `simply-daemon/src/ws/` — WebSocket server
- `simply-daemon/src/remote.rs` — client-side `DaemonApi` impl over WebSocket
- JSON message protocol per [CORE_SERVICE.md](../../../designs/CORE_SERVICE.md)
- Noema/Lumina can swap in-process for remote without code changes

---

## Dependencies

```
Stage 2 (remaining):
  2.4 (stable OAuth port) — independent, small
  2.5 (DocumentApi) → 2.5.1 (rewrite gdocs.rs)
  2.6 (daemon binary) → 2.7 (WebSocket + remote impl)
```
