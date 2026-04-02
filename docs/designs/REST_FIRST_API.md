# REST-First Daemon API

**Status:** refined
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md), [RPC_FRAMEWORK.md](RPC_FRAMEWORK.md)

---

## Problem

Today, all daemon API methods are dispatched over WebSocket JSON-RPC. This has several downsides:

1. **Overly complex for simple operations** — CRUD methods like `list_conversations`, `delete_conversation`, `list_models` don't need a persistent connection. They're request/response.
2. **Admin webpage can't easily call APIs** — the dashboard currently only polls `/admin/api/connections`. To call daemon methods (list sessions, manage MCP servers, etc.) it would need a WebSocket client in the browser.
3. **No standard discoverability** — REST endpoints with proper HTTP methods are self-documenting and can be explored with curl, browser, or any HTTP client.
4. **`/health` and `/kill` are special-cased** — they're hardcoded in the admin module rather than being regular trait methods, creating an inconsistency.
5. **No path to MCP tool exposure** — daemon capabilities can't be offered to agents as tools without manual wiring.

---

## Goals

- **REST-first**: All non-streaming methods become proper HTTP endpoints (GET/POST/PUT/DELETE)
- **WebSocket only for streams**: Session events and voice stay on WebSocket
- **Macro-driven routing**: HTTP method + path derived from trait annotations — no manual route wiring
- **Admin webpage access**: REST endpoints callable directly from the admin dashboard
- **Auth model**: Localhost is trusted (dev mode); remote requires OAuth (admin) or app tokens (lumina/noema)
- **MCP tool generation**: REST methods automatically registered as tools in `McpToolRegistry`
- **DaemonApi trait**: `/health`, `/kill`, and other daemon-level operations become regular REST methods on a trait

## Non-goals

- Replacing WebSocket for streaming use cases
- Public API / third-party consumers (auth is for known clients only)
- Full MCP server implementation — just tool registration in the existing registry

---

## Approach

### 1. New REST annotations on `#[rpc_service]` traits

```rust
#[rpc_service("conversation")]
#[async_trait]
pub trait ConversationApi: Send + Sync {
    #[rpc(get = "/conversation")]
    async fn list_conversations(&self) -> Result<Vec<ConversationInfo>>;

    #[rpc(post = "/conversation")]
    async fn create_conversation(&self, name: &str) -> Result<ConversationId>;

    #[rpc(delete = "/conversation/{id}")]
    async fn delete_conversation(&self, id: &ConversationId) -> Result<()>;

    #[rpc(put = "/conversation/{id}")]
    async fn rename_conversation(&self, id: &ConversationId, name: &str) -> Result<()>;
}
```

**Annotation semantics:**

The path template is written directly in the annotation, matching the URL it produces:

```rust
#[rpc(get    = "/conversation")]                        // GET /conversation
#[rpc(post   = "/conversation")]                        // POST /conversation
#[rpc(delete = "/conversation/{id}")]                   // DELETE /conversation/{id}
#[rpc(put    = "/conversation/{id}")]                   // PUT /conversation/{id}
#[rpc(post   = "/session/{session_id}/message")]        // POST /session/{session_id}/message
#[rpc(get    = "/session/{session_id}/messages")]       // GET /session/{session_id}/messages
```

**Rules:**
- `{name}` segments are matched to method parameters by name
- Remaining parameters (not in the path) come from the request body (POST/PUT) or query string (GET)
- Methods without a REST annotation are **not exposed** as REST endpoints (streaming methods, skipped methods)
- A method can have both `#[rpc(stream)]` and no REST annotation — it stays WebSocket-only
- The `#[rpc_service("prefix")]` on the trait is still used for WebSocket dispatch and MCP tool namespacing, but the REST path is fully explicit per method

### 2. Remove WebSocket dispatch for REST methods

Once a method has a REST annotation, it is **only** available via REST. WebSocket is exclusively for:
- `#[rpc(stream)]` methods (session create/resume/subscribe)
- `#[rpc(skip)]` methods (voice_connect — in-process only)

This simplifies the transport story: REST for request/response, WebSocket for event streams.

### 3. DaemonApi trait for daemon-level operations

```rust
#[rpc_service("daemon")]
#[async_trait]
pub trait DaemonInfoApi: Send + Sync {
    #[rpc(get = "/daemon")]
    async fn health(&self) -> Result<DaemonHealth>;

    #[rpc(post = "/daemon/kill", no_tool)]
    async fn kill(&self) -> Result<()>;

    #[rpc(get = "/daemon/version")]
    async fn version(&self) -> Result<String>;
}
```

This replaces the hardcoded admin routes. The `kill` endpoint sends the shutdown signal through the same channel mechanism, but now it's a regular trait method.

**Note:** The `DaemonApi` super-trait name (currently `= SessionApi + ConversationApi + ...`) may need renaming to avoid confusion. Options: keep `DaemonApi` as the super-trait and call this `DaemonInfoApi`, or rename the super-trait to `AllApis`.

### 4. Codegen changes in `simply-rpc`

The proc macro needs to generate additional metadata and dispatch code:

**New metadata per method:**
```rust
pub struct RestMeta {
    pub http_method: HttpMethod,       // Get, Post, Put, Delete
    pub path_template: &'static str,   // e.g. "/session/{session_id}/message"
    pub method_name: &'static str,
}
```

**New generated code:**
- `ServiceMeta` gains a `rest_methods: &'static [RestMeta]` field
- A `rest_dispatch(&self, http_method, path_segments, query, body) -> Option<Response>` method on the service struct
- The REST server iterates registered services and matches routes from metadata — no manual wiring
- `tool_definitions() -> Vec<ToolDefinition>` on the service struct — descriptions from doc comments, schemas from `RpcSchema` or `schemars`
- `impl_remote_xxx!` generates HTTP client code (reqwest) for REST methods, with path template interpolation and JSON body ser/de. WebSocket code only generated for `stream` methods.

### 5. Auth model

**v1: localhost only.** The REST server binds to `127.0.0.1` — all callers (admin webpage, lumina, noema) are trusted by virtue of being on the same machine. No auth middleware needed.

**Future (post-v1):** Three access levels when remote access is added:

| Caller | Transport | Auth |
|---|---|---|
| Admin (localhost) | REST (browser) | None — trusted |
| Admin (remote) | REST (browser) | OAuth (e.g., GitHub/Google) |
| App (lumina/noema) | REST + WebSocket | App token (shared secret) |

### 6. MCP tool auto-registration

Each REST method auto-generates an `llm::ToolDefinition`:

- **Name:** `{prefix}_{method_name}` (e.g., `conversation_create`)
- **Description:** extracted from `///` doc comments on the trait method
- **Input schema:** auto-derived from method parameters via `schemars::JsonSchema`, with `RpcSchema` trait override for custom types

```rust
#[rpc_service("conversation")]
#[async_trait]
pub trait ConversationApi: Send + Sync {
    /// List all conversations
    #[rpc(get = "/conversation")]
    async fn list_conversations(&self) -> Result<Vec<ConversationInfo>>;

    /// Create a new conversation with the given name
    #[rpc(post = "/conversation")]
    async fn create_conversation(&self, name: &str) -> Result<ConversationId>;
}
// Generates tools: conversation_list_conversations, conversation_create_conversation
// with descriptions from doc comments and schemas from param types
```

**Registration:** At daemon startup, all REST services register their tools into `McpToolRegistry` as a special "daemon" pseudo-server. The `McpToolRegistry` already supports routing tool calls to the right server — daemon tools route to the local trait impl instead of an external MCP connection.

**Opt-out:** `#[rpc(no_tool)]` prevents a method from being exposed as a tool (e.g., `kill` should not be callable by agents). Can be combined with REST annotations: `#[rpc(post = "/daemon/kill", no_tool)]`.

### 7. Admin webpage enhancements

With REST endpoints available, the admin dashboard can:
- List/manage sessions (GET /session, DELETE /session/{id})
- List/manage MCP servers (GET /mcp, POST /mcp, DELETE /mcp/{id})
- View models (GET /model)
- Manage conversations (full CRUD)
- All via simple `fetch()` calls — no WebSocket needed

The admin page at `/` remains served as a special static route (not a trait method).

---

## API mapping (draft)

### ConversationApi (`"conversation"`)

| Method | Annotation | Endpoint |
|---|---|---|
| `list_conversations` | `get = "/conversation"` | `GET /conversation` |
| `create_conversation` | `post = "/conversation"` | `POST /conversation` |
| `delete_conversation` | `delete = "/conversation/{id}"` | `DELETE /conversation/{id}` |
| `rename_conversation` | `put = "/conversation/{id}"` | `PUT /conversation/{id}` |

### SessionApi (`"session"`)

| Method | Annotation | Endpoint |
|---|---|---|
| `create_session` | `stream` | WS only |
| `resume_session` | `stream` | WS only |
| `subscribe_session` | `stream` | WS only |
| `send_message` | `post = "/session/{session_id}/message"` | `POST /session/{session_id}/message` |
| `seed_context` | `post = "/session/{session_id}/context"` | `POST /session/{session_id}/context` |
| `get_messages` | `get = "/session/{session_id}/messages"` | `GET /session/{session_id}/messages` |
| `set_persistence` | `put = "/session/{session_id}/persistence"` | `PUT /session/{session_id}/persistence` |
| `set_model` | `put = "/session/{session_id}/model"` | `PUT /session/{session_id}/model` |
| `close_session` | `delete = "/session/{session_id}"` | `DELETE /session/{session_id}` |
| `close_all_sessions` | `delete = "/session"` | `DELETE /session` |
| `list_sessions` | `get = "/session"` | `GET /session` |
| `reload` | `post = "/session/{session_id}/reload"` | `POST /session/{session_id}/reload` |
| `push_event` | `post = "/session/event"` | `POST /session/event` |

### AssetApi (`"asset"`)

| Method | Annotation | Endpoint |
|---|---|---|
| `store_asset` | `post = "/asset"` | `POST /asset` |
| `get_blob` | `get = "/asset/{hash}"` | `GET /asset/{hash}` |

### McpApi (`"mcp"`)

| Method | Annotation | Endpoint |
|---|---|---|
| `list_mcp_servers` | `get = "/mcp"` | `GET /mcp` |
| `add_mcp_server` | `post = "/mcp"` | `POST /mcp` |
| `remove_mcp_server` | `delete = "/mcp/{server_id}"` | `DELETE /mcp/{server_id}` |
| `connect_mcp_server` | `post = "/mcp/{server_id}/connect"` | `POST /mcp/{server_id}/connect` |
| `disconnect_mcp_server` | `post = "/mcp/{server_id}/disconnect"` | `POST /mcp/{server_id}/disconnect` |
| `get_mcp_server_tools` | `get = "/mcp/{server_id}/tools"` | `GET /mcp/{server_id}/tools` |
| `test_mcp_server` | `post = "/mcp/{server_id}/test"` | `POST /mcp/{server_id}/test` |
| `update_mcp_server_settings` | `put = "/mcp/{server_id}"` | `PUT /mcp/{server_id}` |
| `stop_mcp_retry` | `post = "/mcp/{server_id}/stop-retry"` | `POST /mcp/{server_id}/stop-retry` |
| `start_mcp_retry` | `post = "/mcp/{server_id}/retry"` | `POST /mcp/{server_id}/retry` |

### OAuthApi (`"oauth"`)

| Method | Annotation | Endpoint |
|---|---|---|
| `start_oauth` | `post = "/oauth/{server_id}"` | `POST /oauth/{server_id}` |
| `complete_oauth` | `post = "/oauth/{server_id}/complete"` | `POST /oauth/{server_id}/complete` |
| `complete_oauth_with_code` | `post = "/oauth/{server_id}/code"` | `POST /oauth/{server_id}/code` |
| `resolve_oauth_state` | `get = "/oauth/{state}"` | `GET /oauth/{state}` |

### ModelApi (`"model"`)

| Method | Annotation | Endpoint |
|---|---|---|
| `list_models` | `get = "/model"` | `GET /model` |
| `list_providers` | `get = "/model/provider"` | `GET /model/provider` |
| `default_model_id` | `get = "/model/default"` | `GET /model/default` |
| `set_default_model` | `put = "/model/default"` | `PUT /model/default` |

### VoiceApi (`"voice"`)

| Method | Annotation | Endpoint |
|---|---|---|
| `voice_connect` | `skip` | In-process only |
| `voice_disconnect` | `delete = "/voice/{session_id}"` | `DELETE /voice/{session_id}` |

### DaemonInfoApi (`"daemon"`)

| Method | Annotation | Endpoint |
|---|---|---|
| `health` | `get = "/daemon"` | `GET /daemon` |
| `kill` | `post = "/daemon/kill", no_tool` | `POST /daemon/kill` |
| `version` | `get = "/daemon/version"` | `GET /daemon/version` |

---

## Decided

1. **Tool descriptions** — Doc comments on trait methods. The proc macro extracts `///` comments and includes them in generated `ToolDefinition`. Idiomatic Rust, no extra annotation needed.

2. **Tool input schemas** — Auto-derived from method parameters using `schemars::JsonSchema`. For custom types that don't derive `JsonSchema` or need a different schema, a trait allows overriding:

   ```rust
   /// Implement this to override the auto-derived JSON Schema for a type.
   pub trait RpcSchema {
       fn json_schema() -> schemars::schema::RootSchema;
   }
   ```

   The codegen checks: if a parameter type implements `RpcSchema`, use that; otherwise fall back to `schemars::schema_for!`. This lets you control the schema for opaque types (e.g., `SessionId` → `{"type": "string"}`) without requiring `JsonSchema` on every internal type.

3. **RemoteDaemon client** — `impl_remote_xxx!` generates HTTP client code for REST methods. The macro reads the `RestMeta` (http method + path template) and generates `reqwest` calls with path interpolation, JSON body serialization, and response deserialization. WebSocket connection is only opened when the client calls a `stream` method. `RemoteDaemon` holds both a base URL and a lazy WebSocket connection.

4. **Auth** — Deferred. v1 assumes localhost only. The REST server binds to `127.0.0.1` by default. A future design will add OAuth for remote admin and app tokens for remote client access.

---

## Migration path

1. **Add REST annotations** to existing traits (additive — doesn't break WebSocket yet)
2. **Update codegen** to parse path templates, extract doc comments, generate `RestMeta` + `ToolDefinition` + `rest_dispatch`
3. **Wire REST dispatch** into the existing REST server (auto-route from metadata)
4. **Add DaemonInfoApi** trait, move `/health` and `/kill` out of admin module
5. **Add MCP tool registration** — daemon registers its own REST methods as tools in `McpToolRegistry`
6. **Update `impl_remote_xxx!`** to generate HTTP client code for REST methods
7. **Remove WebSocket dispatch** for REST-annotated methods
8. **Update RemoteDaemon** — hold base URL + lazy WS, use HTTP for REST, WS for streams
9. **Enhance admin webpage** to call REST endpoints directly via `fetch()`
