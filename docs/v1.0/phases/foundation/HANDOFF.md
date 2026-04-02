# Foundation Phase — Handoff

Current state of Stage 2 (Daemon) and open decisions for the next session.

---

## Completed

- **Stage 1** — Workspace restructure (all done)
- **2.1** — DaemonApi split into 7 focused traits: SessionApi, ConversationApi, AssetApi, McpApi, OAuthApi, ModelApi, VoiceApi
- **2.2** — EmbeddedDaemon (in-process impl) — all traits implemented
- **2.3** — Noema wired to daemon: chat, conversations, models, assets, MCP, OAuth all route through daemon traits
- **2.3.1** — Noema decoupled from simply-core/llm, renamed to `noema/`
- **2.3.2** — MCP commands + OAuth moved into daemon (McpApi + OAuthApi + OAuthService)

## Remaining (no design blockers)

- **2.4** — Stable OAuth callback port (daemon starts long-lived server on configured port)
- **2.6** — Daemon binary (startup, config, shutdown)
- **2.7** — WebSocket server + remote DaemonApi (lets Lumina connect)

## Blocked on design decision

- **2.5** — DocumentApi + gdocs rewrite (see open design problem below)

---

## Open Design Problem: Domain Features as MCP Sidecars

### Context

Google Docs support currently lives partly in `noema-mcp-gdocs` (Google API client) and partly in Noema's `gdocs.rs` commands (storage, indexing, UI orchestration). The `gdocs.rs` file is broken — it uses `EmbeddedDaemon` escape hatches (`stores()`, `coordinator()`) that should be removed.

### Proposed direction

Domain-specific features (Google Docs, future integrations) should be **MCP sidecars** — standalone processes that the daemon connects to as MCP servers. The daemon stays generic.

### What this means

1. **`noema-mcp-gdocs`** stays a standalone MCP server (sidecar). It handles Google OAuth, fetching, and extracting documents. It returns content to the caller — it does not write to storage.

2. **The daemon needs `invoke_tool`** — a way for clients (Noema UI, Lumina Discord) to call MCP tools directly without going through an LLM conversation turn. This is the missing piece that makes sidecars useful for user-initiated actions like "import this doc."

3. **The daemon still needs a DocumentApi** for CRUD on documents stored in the UCM. Tools like `list_documents`, `get_document`, `delete_document` are daemon concerns regardless of where the doc came from.

4. **The open question is orchestration**: who connects fetch → store?
   - **Option A: Client orchestrates.** Noema/Lumina calls `invoke_tool("gdocs", "fetch_document", {id})`, gets content back, then calls `daemon.import_document(content)`. Simple, but duplicates orchestration across clients.
   - **Option B: Daemon orchestrates.** `daemon.import_document(source: "gdocs", id: "...")` — daemon calls the sidecar internally, then stores. Cleaner for clients, but daemon needs to know about import sources.
   - **Option C: Sidecar calls back.** Sidecar fetches the doc and calls a daemon tool (via MCP) to store it. Daemon exposes `store_document` as an MCP tool. Clean separation, but adds complexity and bidirectional MCP.

### What we need to decide

- Which orchestration model (A/B/C)?
- Does `invoke_tool` belong on `McpApi` or a new trait?
- How does the sidecar get Google OAuth tokens — daemon's OAuthService, or self-managed?
- Should `invoke_tool` be typed (returns structured results) or generic (returns `serde_json::Value`)?

### What can proceed without this decision

Everything except 2.5 (DocumentApi) and 2.5.1 (gdocs rewrite). The daemon binary, WebSocket server, and stable OAuth port are all independent.

---

## Other Open Items

### Voice recognition -> daemon
Voice recognition currently lives in Noema (`simply-audio` + `VoiceCoordinator` on `AppState`). Should move into the daemon so any client (Lumina, etc.) can use it. The `VoiceApi` trait is stubbed and waiting.

### EmbeddedDaemon escape hatches
`EmbeddedDaemon` still exposes `stores()`, `coordinator()`, `mcp_registry()` as pub accessors for gdocs commands. These should be removed once the sidecar/DocumentApi design is resolved.
