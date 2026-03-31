# Foundation Phase — Snapshot

**Date:** 2026-03-31
**Stage:** 2 (Daemon)
**Status:** In progress — task 2.3.1 nearly complete, 2.3.2 next

---

## What's Done

### Stage 1 (Complete)
All workspace restructure tasks done: renamed crates, created simply-daemon, merged noema-mcp-core, renamed noema-desktop -> noema.

### Stage 2 Progress

**2.1 DaemonApi trait** — Done. Split into 6 focused traits in `simply-daemon/src/api/`:
- `SessionApi` — session lifecycle, event streaming via `broadcast::Receiver<DaemonEvent>`
- `ConversationApi` — persistent conversation CRUD
- `AssetApi` — binary content storage (`store_asset`, `get_blob`)
- `McpApi` — full MCP CRUD (list/add/remove/connect/disconnect/test/update/retry)
- `ModelApi` — `list_models`, `list_providers`, `default_model_id`, `set_default_model`
- `VoiceApi` — stubbed
- `DaemonApi` super-trait auto-implemented for anything implementing all 6

**2.2 In-process implementation** — Done. `EmbeddedDaemon<S: StorageTypes>` in `embedded.rs`:
- Generic over storage, takes `Arc<dyn Stores<S>>` internally
- Constructor `new<T: Stores<S> + 'static>(stores: Arc<T>)` — creates coordinator, resolves user, creates MCP registry, starts daemon MCP server, starts auto-connect
- Per-session `ManagedSession` with `broadcast::Sender<DaemonEvent>` for multi-listener streaming
- Event dispatcher task routes `(ConversationId, ManagerEvent)` -> per-session broadcasts
- All 6 API traits implemented
- Temporary accessor methods (`mcp_registry()`, `stores()`, `coordinator()`) for gdocs (not yet moved to daemon API)

**2.3 Wire Noema to in-process daemon** — Done for chat/conversations/models/assets.

**2.3.1 Decouple Noema from simply-core/llm** — ~90% done:
- `noema/src-tauri/Cargo.toml` no longer depends on `simply-core` or `llm`
- `chat.rs` — fully routes through daemon (sessions, conversations, models, messages)
- `init.rs` — minimal: `SqliteStores::open()` -> `EmbeddedDaemon::new()` -> done
- `state.rs` — simplified: only holds `OnceCell<Arc<AppDaemon>>`, no stores/coordinator/registry
- `types.rs` — imports from `simply_daemon::types` only
- `lib.rs` — asset protocol uses `daemon.get_blob()`
- Frontend cleaned: removed fork/regenerate/edit/privacy/subconversation features
- **Broken files remaining:** `mcp.rs` and `gdocs.rs` (see below)

---

## What's Broken / In Progress

### `noema/src-tauri/src/commands/mcp.rs` (task 2.3.2)
**Status:** Compiles against old API that no longer exists.

Every command calls `state.get_mcp_registry()` which was removed. Needs full rewrite to go through `McpApi` trait methods on the daemon.

The file has two layers of work:
1. **CRUD commands** (~15 Tauri commands): `list_mcp_servers`, `add_mcp_server`, `remove_mcp_server`, `connect_mcp_server`, `disconnect_mcp_server`, `get_mcp_server_tools`, `test_mcp_server`, `update_mcp_server_settings`, `stop_mcp_retry`, `start_mcp_retry`. These should become thin wrappers around `daemon.list_mcp_servers()`, etc. The `McpApi` trait already has methods for all of these.

2. **OAuth flow** (~400 lines): `start_mcp_oauth`, `complete_mcp_oauth`, `complete_oauth_internal`, `exchange_code_for_tokens`, `save_oauth_tokens`, `fetch_well_known`, `register_oauth_client`, `handle_deep_link`. **All of this must move into the daemon** — the user was explicit that OAuth is a daemon concern, not a Noema concern. New methods needed on `McpApi`:
   - `start_oauth(server_id) -> OAuthFlowInfo` (returns auth URL + state param)
   - `complete_oauth(server_id, code, state) -> Result<()>` (exchanges code, saves tokens, reconnects)
   - `fetch_well_known(server_url) -> WellKnownConfig`
   - Pending OAuth state tracking moves into daemon (currently on `AppState.pending_oauth_states`)

Also: `oauth_callback.rs` (local HTTP callback server) should move into the daemon.

### `noema/src-tauri/src/commands/gdocs.rs` (task 2.3.3, P1)
**Status:** Broken. References `state.get_stores()` and `state.get_coordinator()` which no longer exist. Paused — lower priority. Temporarily uses accessor methods on `EmbeddedDaemon` as escape hatch until a `DocumentApi` is built.

---

## Key Architecture Decisions

1. **Daemon is the single API boundary.** Noema/Lumina never reach into simply-core internals. All access goes through daemon traits.

2. **Storage is orthogonal to hosting.** `EmbeddedDaemon` (in-process) and future remote daemon both work with any storage backend. Storage choice is a config decision, not a build decision.

3. **Single binary, runtime config.** No feature flags for embedded vs remote. One build target, command-line argument picks mode.

4. **Per-session broadcast channels.** `create_session`/`resume_session` return `(SessionInfo, broadcast::Receiver<DaemonEvent>)`. Multiple listeners can subscribe. Noema bridges these to Tauri events via forwarder tasks.

5. **Forwarder tracking.** `AppState.forwarders: HashMap<String, JoinHandle<()>>` — aborts old forwarder when session is re-loaded to avoid duplicates.

6. **SessionId is a newtype**, not a String alias.

7. **`DaemonMcpServer`** (renamed from `NoemaCoreServer`) in `simply-daemon/src/mcp/tools.rs` — the daemon's own MCP server that exposes tools to external MCP clients.

---

## Open Design Questions / Future Work

### OAuth callback server port stability
The current `oauth_callback.rs` starts on a random port. If Lumina runs in the cloud and OAuth happens remotely, the callback server needs a stable, known port (or a different redirect strategy like a fixed URL/reverse proxy). This affects the daemon's OAuth implementation.

### Voice recognition -> daemon
Voice recognition currently lives in Noema (`simply-audio` + `VoiceCoordinator` on `AppState`). This should move into the daemon so Lumina (or any client) can use voice. The `VoiceApi` trait is stubbed and waiting for this. Involves:
- Moving `VoiceCoordinator` into the daemon
- Exposing voice start/stop/status through `VoiceApi`
- Noema becomes a thin bridge (microphone capture -> daemon, daemon events -> audio playback)

### Remaining Stage 2 tasks (not started)
- **2.4** Session manager: in-memory state, ephemeral + persistent modes
- **2.5** Daemon binary: startup, config loading, graceful shutdown
- **2.6** WebSocket server + remote `DaemonApi` implementation
- **2.7** REST server: `/events`, `/register`, `/health`
- **2.8** Peer registry: connected clients, global MCP tool registry
- **2.9** MCP client: connect to action services, discover tools
- **2.10** Move storage from `simply-core` -> `simply-daemon`

---

## File Reference

Key files touched/created during this stage:

| File | Status |
|------|--------|
| `simply-daemon/src/api/mod.rs` | Done — trait definitions |
| `simply-daemon/src/api/session.rs` | Done |
| `simply-daemon/src/api/conversation.rs` | Done |
| `simply-daemon/src/api/asset.rs` | Done |
| `simply-daemon/src/api/mcp.rs` | Done (needs OAuth methods added) |
| `simply-daemon/src/api/model.rs` | Done |
| `simply-daemon/src/api/voice.rs` | Stubbed |
| `simply-daemon/src/api/types.rs` | Done — re-exports all client-facing types |
| `simply-daemon/src/embedded.rs` | Done (implements all traits) |
| `simply-daemon/src/storage.rs` | Done — `SqliteStores::open()` |
| `simply-daemon/src/mcp/tools.rs` | Renamed to `DaemonMcpServer` |
| `noema/src-tauri/src/commands/chat.rs` | Done — routes through daemon |
| `noema/src-tauri/src/commands/init.rs` | Done — minimal |
| `noema/src-tauri/src/state.rs` | Done — simplified |
| `noema/src-tauri/src/commands/mcp.rs` | **Broken** — needs rewrite (2.3.2) |
| `noema/src-tauri/src/commands/gdocs.rs` | **Broken** — paused (2.3.3) |
| `noema/src-tauri/src/oauth_callback.rs` | Needs to move into daemon |
| `noema/src-tauri/src/types.rs` | Done — uses daemon types |
| `noema/src-tauri/src/lib.rs` | Done — asset protocol via daemon |

---

## How to Resume

1. Start with task **2.3.2** — rewrite `mcp.rs`:
   - Add OAuth methods to `McpApi` trait (`start_oauth`, `complete_oauth`)
   - Move `oauth_callback.rs`, `fetch_well_known`, `exchange_code_for_tokens`, `save_oauth_tokens`, `register_oauth_client` into the daemon
   - Move `pending_oauth_states` from `AppState` into the daemon
   - Rewrite all Tauri MCP commands as thin wrappers around daemon API
   - Handle `handle_deep_link` — Noema receives the deep link URL but passes it to daemon for processing

2. Then mark **2.3.1** complete and **2.3.2** complete in TASKS.md.

3. Decide on **gdocs** (2.3.3) or move to **2.4** (session manager).
