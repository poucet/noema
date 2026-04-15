# Core Service — Communication & RPC Framework

**Status:** Implemented (Stage 3 in progress)
**Version:** 1.0
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)
**Crate:** `simply-rpc/` (+ `simply-rpc/macros/`)

---

## Overview

`simply-daemon` is the hub. It owns agent orchestration (via `simply-core`), UCM storage, event/intent engine, voice pipeline, and session management. All communication flows through a single port:

- **REST** — request/response operations (CRUD, configuration, queries). Auto-generated from trait annotations.
- **WebSocket** — streaming sessions and bidirectional streams. Client upgrades at stream paths, receives events, can send messages.
- **MCP outbound** — action services that the daemon connects to and calls tools on.

Additionally, REST methods are exposed as **in-process tools** via the `ToolService` trait, so agents see daemon capabilities and external MCP tools identically.

`simply-core` is a library crate internal to the daemon: LLM providers, MCP client/server, agent orchestration. No external crate depends on it.

---

## Architecture

```
simply-daemon (single port)
├─ simply-core (internal library)
│   ├─ LLM providers
│   ├─ MCP client/server
│   └─ Agent orchestration
├─ UCM storage (SQLite, blobs)
├─ Session manager (in-memory conversation state)
├─ Global MCP tool registry (external MCP + daemon tools via ToolService)
├─ Event bus + intent engine
├─ Voice pipeline (simply-voice)
│
├─ REST — request/response (all HTTP methods)
│   ▲           ▲           ▲           ▲
│   Noema       Lumina      Admin page  Trigger services
│
├─ WebSocket — streaming sessions + bidi streams
│   ▲           ▲
│   Noema       Lumina
│
└─ MCP outbound — action services
    ▼           ▼
    github      any MCP server
    watcher
```

---

## The `simply-rpc` Framework

A proc macro `#[rpc_service("prefix")]` annotates async traits and auto-generates:

1. **REST dispatch** — routes HTTP requests to trait methods based on path annotations
2. **WebSocket dispatch** — handles stream methods with bidirectional support
3. **Route metadata** — `RouteMeta` entries for auto-routing and tool generation
4. **Tool definitions** — `ToolDefinition` for each REST method (descriptions from doc comments, schemas from params)
5. **Client impl macro** — `impl_remote_xxx!` generates HTTP/WS client code
6. **Compatibility metadata** — per-method signature hashes for version checking

### Annotating a trait

```rust
#[rpc_service("conversation")]
#[async_trait]
pub trait ConversationApi: Send + Sync {
    /// List all conversations
    #[rpc(get = "/conversation")]
    async fn list_conversations(&self) -> Result<Vec<ConversationInfo>>;

    /// Create a new conversation
    #[rpc(post = "/conversation")]
    async fn create_conversation(&self, name: Option<String>) -> Result<ConversationId>;

    /// Delete a conversation by ID
    #[rpc(delete = "/conversation/{id}")]
    async fn delete_conversation(&self, id: &ConversationId) -> Result<()>;
}
```

### Annotations

| Annotation | Effect |
|---|---|
| `#[rpc(get = "/path")]` | HTTP GET at the given path |
| `#[rpc(post = "/path")]` | HTTP POST at the given path |
| `#[rpc(put = "/path")]` | HTTP PUT at the given path |
| `#[rpc(delete = "/path")]` | HTTP DELETE at the given path |
| `#[rpc(stream = "/path")]` | WebSocket stream at the given path (bidirectional) |
| `#[rpc(skip)]` | Not dispatched; client gets `bail!()` stub (in-process only) |
| `#[rpc(no_tool)]` | Exclude from tool generation. Combinable with other annotations. |
| `#[rpc(base64_param = "name")]` | Named `Vec<u8>` param encoded as base64 over the wire |
| `#[rpc(base64_return)]` | Return value encoded as base64 over the wire |

### Path templates

```rust
#[rpc(get    = "/conversation")]                    // GET /conversation
#[rpc(delete = "/conversation/{id}")]               // DELETE /conversation/{id}
#[rpc(post   = "/session/{session_id}/message")]    // POST /session/{session_id}/message
#[rpc(stream = "/session/new")]                     // WS stream at /session/new
#[rpc(stream = "/voice/stream/{provider_id}")]      // WS stream with path params
```

- `{name}` segments match method parameters by name
- Remaining parameters come from the request body (POST/PUT) or query string (GET)
- Stream methods accept WebSocket upgrade; all streams are bidirectional

### Stream return types

Stream methods support three return patterns:

```rust
// Tuple: initial value + event stream (e.g., sessions)
#[rpc(stream = "/session/new")]
async fn create_session(&self, opts: CreateSessionOptions)
    -> Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)>;

// Bare: event stream only
#[rpc(stream = "/session/{id}/subscribe")]
async fn subscribe_session(&self, id: &SessionId)
    -> Result<broadcast::Receiver<DaemonEvent>>;

// StreamHandle: fully bidirectional (e.g., voice)
#[rpc(stream = "/voice/stream/{provider_id}")]
async fn voice_connect(&self, provider_id: &str)
    -> Result<StreamHandle<VoiceInput, VoiceEvent>>;
```

`StreamHandle<T, U>` is a bidirectional channel:
- `T` = messages the client sends (serialized as JSON)
- `U` = messages the server sends (serialized as JSON)
- Binary data uses `#[serde(with = "simply_rpc::base64_bytes")]` for base64 encoding

The macro generates all WS bridging code — forwarders for server→client events, deserializers for client→server input.

### Transport routing

All on a **single port**. The `ServiceRouter` checks the request:

1. If `Upgrade: websocket` header and path matches a `stream` annotation → WebSocket upgrade
2. Otherwise → REST dispatch by HTTP method + path

A method is on exactly one transport:
- **REST** — `get`, `post`, `put`, `delete` annotations
- **WebSocket** — `stream` annotation (bidirectional)
- **In-process only** — `skip` (rare, for non-serializable types)

---

## REST — Request/Response

All non-streaming operations. Auto-routed from trait annotations via `ServiceRouter`.

### Characteristics

- **Standard HTTP** — GET/POST/PUT/DELETE with JSON bodies. Curl-friendly, browser-friendly.
- **Macro-driven routing** — paths declared in annotations, no manual route wiring.
- **Admin-friendly** — the admin webpage calls the same endpoints via `fetch()`.

### Auth (v1)

**Localhost only.** The server binds to `127.0.0.1` — all callers are trusted.

**Future (post-v1):** Localhost remains trusted. Remote access adds OAuth for admin and app tokens for Noema/Lumina.

---

## WebSocket — Streaming & Bidirectional

Sessions and voice use WebSocket for real-time communication. Each stream is its own logical connection over the shared WS.

### Session lifecycle

| Path | Method | What happens |
|---|---|---|
| `ws://host/session/new` | `create_session` | Create new session. Server sends `SessionInfo`, then streams `DaemonEvent`. |
| `ws://host/session/{id}/subscribe` | `subscribe_session` | Subscribe to events. Server streams `DaemonEvent`. |

### Voice streams

| Path | Method | What happens |
|---|---|---|
| `ws://host/voice/stream/{provider_id}` | `voice_connect` | Bidirectional voice stream. Client sends `VoiceInput`, server sends `VoiceEvent`. |

### Bidirectional protocol

All streams are bidirectional. Messages flow as JSON text frames:

- **Server → Client:** notifications with `{ "method": "voice.voice_connect.event", "params": ... }`
- **Client → Server:** notifications with `{ "method": "voice.voice_connect.input", "params": ... }`

For `StreamHandle<T, U>` methods, the macro generates:
- A deserializer that converts incoming JSON to `T` and sends to the service
- A forwarder that serializes `U` events as JSON notifications to the client

### Context seeding

Context is passed in `CreateSessionOptions.context` during session creation — no separate `seed_context` method.

---

## MCP Outbound — Action Services

For services that expose tools the daemon's agent can call. The daemon connects to them as an MCP client.

1. Service starts up and exposes an MCP server (standard MCP protocol)
2. Service registers with the daemon via REST: `POST /mcp` with endpoint URL
3. Daemon connects, discovers tools → available in the global tool registry
4. When the agent needs a tool → daemon invokes it via the MCP connection
5. Connection drops → tools become unavailable → actions deferred until reconnection

Services can also be configured declaratively as UCM documents with `type: mcp_server` frontmatter.

A service can be both trigger (pushes events via `POST /session/event`) and action (exposes tools via MCP).

---

## Daemon as Tool Provider

REST methods on daemon traits are automatically exposed as tools via the `ToolService` trait:

```rust
pub trait ToolService: Send + Sync {
    async fn get_definitions(&self) -> Vec<ToolDefinition>;
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Vec<ToolResultContent>>;
}
```

**In-process only** — no MCP server, no port. The daemon generates a `ToolService` impl from its REST-annotated trait methods. Registered in `McpToolRegistry` alongside external MCP tools.

- **Tool name:** `{prefix}_{method_name}` (e.g., `conversation_list_conversations`)
- **Description:** from `///` doc comments on the trait method
- **Input schema:** auto-derived from parameters via `schemars::JsonSchema`, with `RpcSchema` trait for overrides
- **Opt-out:** `#[rpc(no_tool)]` excludes a method (e.g., `kill`)

---

## MCP Tool Registry — Global and Shared

All tools in a single registry, regardless of source:

- **Daemon tools** (via `ToolService`): conversation CRUD, session management, model queries
- **Client-registered tools** (via WebSocket `RegisterMcp`): Discord tools from Lumina, filesystem tools from Noema
- **Service-registered tools** (via MCP outbound): GitHub tools, any external MCP server

**All tools are shared by default.** Platform-specific tools are globally visible; action routing defers if the platform is disconnected.

---

## API Surface

### SessionApi (`session`)

| Method | Annotation | Endpoint |
|---|---|---|
| `create_session` | `stream = "/session/new"` | WS stream |
| `subscribe_session` | `stream = "/session/{session_id}/subscribe"` | WS stream |
| `list_sessions` | `get = "/session"` | GET |
| `send_message` | `post = "/session/{session_id}/message"` | POST |
| `set_model` | `put = "/session/{session_id}/model"` | PUT |
| `close_session` | `delete = "/session/{session_id}"` | DELETE |
| `close_all_sessions` | `delete = "/session"` | DELETE |
| `push_event` | `post = "/session/event"` | POST |

### ConversationApi (`conversation`)

| Method | Annotation | Endpoint |
|---|---|---|
| `list_conversations` | `get = "/conversation"` | GET |
| `create_conversation` | `post = "/conversation"` | POST |
| `delete_conversation` | `delete = "/conversation/{id}"` | DELETE |
| `rename_conversation` | `put = "/conversation/{id}"` | PUT |

### AssetApi (`asset`)

| Method | Annotation | Endpoint |
|---|---|---|
| `store_asset` | `post = "/asset"` | POST |
| `get_blob` | `get = "/asset/{hash}"` | GET |

### McpApi (`mcp`)

| Method | Annotation | Endpoint |
|---|---|---|
| `list_mcp_servers` | `get = "/mcp"` | GET |
| `add_mcp_server` | `post = "/mcp"` | POST |
| `remove_mcp_server` | `delete = "/mcp/{server_id}"` | DELETE |
| `connect_mcp_server` | `post = "/mcp/{server_id}/connect"` | POST |
| `disconnect_mcp_server` | `post = "/mcp/{server_id}/disconnect"` | POST |
| `get_mcp_server_tools` | `get = "/mcp/{server_id}/tools"` | GET |
| `update_mcp_server_settings` | `put = "/mcp/{server_id}"` | PUT |
| `stop_mcp_retry` | `post = "/mcp/{server_id}/stop-retry"` | POST |
| `start_mcp_retry` | `post = "/mcp/{server_id}/retry"` | POST |
| `list_all_tools` | `get = "/mcp/tools"` | GET (user-scoped via RequestContext) |
| `call_tool_direct` | `post = "/mcp/tools/call"` | POST (user-scoped via RequestContext) |

### OAuthApi (`oauth`)

| Method | Annotation | Endpoint |
|---|---|---|
| `start_oauth` | `post = "/oauth/{server_id}"` | POST |
| `complete_oauth` | `post = "/oauth/{server_id}/complete"` | POST |
| `complete_oauth_with_code` | `post = "/oauth/{server_id}/code"` | POST |
| `resolve_oauth_state` | `get = "/oauth/{state}"` | GET |

### ModelApi (`model`)

| Method | Annotation | Endpoint |
|---|---|---|
| `list_models` | `get = "/model"` | GET |
| `list_providers` | `get = "/model/provider"` | GET |
| `default_model_id` | `get = "/model/default"` | GET |
| `set_default_model` | `put = "/model/default"` | PUT |

### VoiceApi (`voice`)

| Method | Annotation | Endpoint |
|---|---|---|
| `list_voice_providers` | `get = "/voice/provider"` | GET |
| `voice_connect` | `stream = "/voice/stream/{provider_id}"` | WS bidi stream |
| `voice_disconnect` | `delete = "/voice/{session_id}"` | DELETE |

### CoreApi (`core`)

| Method | Annotation | Endpoint |
|---|---|---|
| `health` | `get = "/daemon"` | GET |
| `kill` | `post = "/daemon/kill", no_tool` | POST |
| `version` | `get = "/daemon/version"` | GET |

---

## Internals

### Server wiring

```rust
// ServiceRouter handles both REST and WS stream routing
let router = Arc::new(ServiceRouter::new()
    .register(<dyn SessionApi>::service(daemon.clone()))
    .register(<dyn ConversationApi>::service(daemon.clone()))
    .register(<dyn VoiceApi>::service(daemon.voice_service()))
    // ... all services
);

// Single port serves everything:
// 1. WS upgrade + stream path match → bidirectional stream
// 2. WS upgrade at /ws → JSON-RPC (session commands)
// 3. Otherwise → REST dispatch
```

### Client wiring

```rust
// RemoteDaemon holds base URL + lazy WS connections
impl_remote_session_api!(RemoteDaemon);      // HTTP for REST, WS for stream
impl_remote_conversation_api!(RemoteDaemon); // HTTP only (no streams)
impl_remote_voice_api!(RemoteDaemon);        // HTTP for REST, WS bidi for stream
// ... one line per trait
```

### Compatibility metadata

`ServiceMeta` includes per-method signature hashes. At connection time, client and server exchange metadata to detect version mismatches.

---

## Connection Resilience

REST calls are stateless and retry-friendly. WebSocket connections auto-reconnect with exponential backoff (100ms → 30s cap).

On daemon restart:
- UCM-backed sessions are reloadable from storage
- Ephemeral sessions are lost — clients re-seed from their own state
- MCP tool registrations are re-established on reconnect

---

## Adding a New Rich Client

1. Handle platform-specific I/O (gateway, commands, audio)
2. Use `RemoteDaemon` (REST + WS) — public trait API unchanged
3. Register platform-specific MCP tools (shared globally)
4. Register platform-specific event sources
5. Handle session events via WebSocket subscription

## Adding a New Integration Service

1. Expose an MCP server with your tools
2. Register via `POST /mcp` with your endpoint
3. Optionally push events via `POST /session/event`
4. That's it — tools available to all sessions

---

## Crate structure

```
simply-rpc/
  macros/src/
    lib.rs                 # #[rpc_service("prefix")] proc macro entry
    parse.rs               # Parse annotations, classify params/returns
    codegen_dispatch.rs    # Generate service struct, RpcService, RestService, WS dispatch, metadata
    codegen_client.rs      # Generate impl_remote_xxx! macro
  src/
    lib.rs                 # Re-exports
    service.rs             # RpcService, RestService, ServiceRouter
    client.rs              # RpcClient trait
    context.rs             # DispatchResult<S>
    stream.rs              # StreamHandle<T, U> — bidirectional stream type
    helpers.rs             # call_unit, call_val, call_raw, base64
    meta.rs                # ServiceMeta, MethodMeta, RouteMeta, HttpMethod, check_compat

simply-daemon/src/
  remote.rs                # RemoteDaemon — base URL + lazy WS
  net/
    rest.rs                # HTTP + WebSocket server (single port, unified handler)
    server.rs              # ConnectionTracker
    discovery.rs           # connect_or_host()
  api/
    session.rs             # SessionApi — stream + REST
    conversation.rs        # ConversationApi — REST
    asset.rs               # AssetApi — REST
    mcp.rs                 # McpApi — REST
    oauth.rs               # OAuthApi — REST
    model.rs               # ModelApi — REST
    voice.rs               # VoiceApi — REST + bidi stream
    core.rs                # CoreApi — REST
  main.rs                  # Service wiring
```
