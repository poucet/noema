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
