# Request Scope

**Status:** draft
**Version:** 1.0
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)
**Related:** [AUTH_AND_IDENTITY.md](AUTH_AND_IDENTITY.md), [GOOGLE_DOCS_IMPORT.md](GOOGLE_DOCS_IMPORT.md)

---

## Problem

Services have no concept of "who is making this request." The auth middleware resolves the user from `X-User-Id` headers, but that information dies at the HTTP boundary — it never reaches service methods like `call_tool_direct()` or `list_documents()`.

This blocks:
- Per-user MCP connections (Google Docs OAuth tokens scoped to user)
- Per-user document ownership (currently hardcoded to admin)
- Per-user tool visibility (authenticated users see more tools)
- Any future per-user behavior

## Goals

- Every service method has access to the calling user's identity
- Anonymous users (random Discord chatters) have zero overhead — no DB lookups
- Authenticated users get scoped behavior (their MCP connections, their docs)
- One code path — no `daemon.scoped()` vs `daemon.document()` split
- Scope is extensible (user_id today, permissions/session_id/etc. later)
- Works for all callers: HTTP (Lumina, admin UI), embedded (Noema), WebSocket

## Non-Goals

- Permission system (roles, ACL) — future, built on top of scope
- Per-request database connections or transactions
- Request tracing/telemetry (could use scope later, but not the goal now)

---

## Design

### Scope

A lightweight value carried on every request:

```rust
#[derive(Clone, Debug)]
pub struct Scope {
    /// None = anonymous/default, Some = authenticated user
    pub user_id: Option<UserId>,
}

impl Scope {
    pub fn anonymous() -> Self {
        Self { user_id: None }
    }

    pub fn user(user_id: UserId) -> Self {
        Self { user_id: Some(user_id) }
    }
}
```

Cheap to create — just an `Option<UserId>`. No DB lookup, no connection setup. The expensive work (resolving per-user MCP connections) only happens when a service needs it and `user_id` is `Some`.

### How Scope Reaches Services

**HTTP path (Lumina, admin UI):**
1. Auth middleware resolves user from `X-User-Id` header (or session cookie)
2. Creates `Scope` and attaches to request extensions (already done for `RequestUser`)
3. RPC dispatch macro extracts `Scope` from request extensions
4. Passes it to the service method

**Embedded path (Noema):**
1. `EmbeddedDaemon` holds the admin's `Scope` (set at startup)
2. All method calls use this scope
3. No middleware — scope is part of the daemon instance

**WebSocket path:**
1. WS connection carries user identity (from initial handshake headers)
2. All messages on that connection use the same scope

### Service Method Signature

Service methods that need user context receive `Scope` as a parameter. The RPC macro injects it automatically from the request context.

**Option A — Scope on every method:**
```rust
#[rpc_service("document")]
pub trait DocumentApi {
    #[rpc(get = "/document")]
    async fn list_documents(&self, scope: Scope) -> Result<Vec<DocumentInfo>>;
}
```

**Option B — Scope on the service instance (scoped factory):**
```rust
// Service holds scope, methods don't take it
pub struct DocumentService {
    stores: Arc<dyn Stores>,
    scope: Scope,
}

impl DocumentService {
    fn user_id(&self) -> Option<&UserId> {
        self.scope.user_id.as_ref()
    }
}
```

**Option C — Scope as thread-local / task-local context:**
```rust
// Set once per request, readable anywhere in the call stack
tokio::task_local! {
    static SCOPE: Scope;
}
```

### Recommended: Option A

Scope as an explicit parameter. Reasons:
- Visible in the type signature — clear which methods are scope-aware
- No hidden state (task-local is surprising)
- No per-request service construction (Option B creates garbage)
- The RPC macro can inject it automatically — callers don't have to pass it manually
- For embedded (Noema), the `EmbeddedDaemon` impl passes the stored scope

The RPC macro change: when dispatching a request, extract `Scope` from the request extensions. If the method signature includes a `Scope` parameter, inject it. If not, the method is scope-agnostic (e.g. `health()`, `version()`).

### Impact on Existing Services

Services that currently use `self.user_id` (like `DocumentService`) switch to getting the user from the `Scope` parameter:

```rust
// Before
async fn list_documents(&self) -> Result<Vec<DocumentInfo>> {
    let docs = self.stores.document().list_documents(&self.user_id).await?;
    // ...
}

// After
async fn list_documents(&self, scope: Scope) -> Result<Vec<DocumentInfo>> {
    let user_id = scope.user_id.as_ref()
        .ok_or_else(|| anyhow!("authentication required"))?;
    let docs = self.stores.document().list_documents(user_id).await?;
    // ...
}
```

For MCP tool calls:
```rust
async fn call_tool_direct(&self, scope: Scope, request: CallToolRequestParam) -> Result<CallToolResult> {
    // scope.user_id determines which MCP connections to use
    let tools = match &scope.user_id {
        Some(uid) => self.user_tools.get(uid).await?,
        None => self.global_tools.clone(),
    };
    tools.call_tool(&request.name, ...).await
}
```

### How Lumina Creates Scopes

- **Anonymous chat:** `Scope::anonymous()` — no DB lookup, no overhead
- **Authenticated user:** Lumina resolves `discord:{id}` → UCM user_id once (via `UserApi::resolve_or_create_user`), caches the mapping, creates `Scope::user(ucm_id)` for subsequent requests
- **The scope is sent per-request** via `X-User-Id` header — already supported

### RPC Macro Changes

The `#[rpc_service]` macro needs to:
1. Detect if a method has a `Scope` parameter
2. If yes: extract `Scope` from request extensions during dispatch, pass it to the method
3. If no: dispatch as before (scope-agnostic method)

This is backward-compatible — existing methods without `Scope` continue working.

---

## Migration Path

1. Add `Scope` struct to `simply-daemon` (or `simply-rpc`)
2. Update RPC macro to support `Scope` injection
3. Add `Scope` to request extensions in auth middleware (alongside existing `RequestUser`)
4. Migrate `DocumentService` — remove `self.user_id`, take `Scope` parameter
5. Migrate `McpApi::call_tool_direct` — use scope to resolve per-user tools
6. Migrate `SearchService` — scope the search to the user's documents
7. Update `EmbeddedDaemon` to pass admin scope on all calls

Steps 1-3 are the infrastructure. Steps 4-7 are incremental — each service migrates independently.

## Open Questions

1. **Where does `Scope` live?** `simply-rpc` (since the macro needs it), `simply-daemon`, or a new shared crate?
2. **Should `EmbeddedDaemon` hold a `Scope`?** Or should each call site (Noema Tauri commands) pass it explicitly?
