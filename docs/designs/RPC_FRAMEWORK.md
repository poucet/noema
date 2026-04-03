# simply-rpc — Generic Trait-over-Network RPC Framework

**Status:** Implemented (REST extensions: planned)
**Crate:** `simply-rpc/` (+ `simply-rpc/macros/`)
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)

---

## Overview

A proc macro `#[rpc_service("prefix")]` annotates async traits and auto-generates:
1. A **server service struct** implementing `RpcService` — dispatches JSON-RPC calls to trait methods
2. A **client impl macro** — implements the trait for any type implementing `RpcClient`
3. **REST dispatch** — routes HTTP requests to trait methods based on path template annotations
4. **Tool definitions** — generates `ToolDefinition` for each REST method (descriptions from doc comments, schemas from params)

The framework is **generic** — it knows nothing about daemons, sessions, or LLMs. The daemon is just one consumer.

---

## Usage

### Annotating a trait

```rust
#[rpc_service("conversation")]
#[async_trait]
pub trait ConversationApi: Send + Sync {
    /// List all conversations
    #[rpc(get = "/conversation")]
    async fn list_conversations(&self) -> anyhow::Result<Vec<ConversationInfo>>;

    /// Create a new conversation with the given name
    #[rpc(post = "/conversation")]
    async fn create_conversation(&self, name: &str) -> anyhow::Result<ConversationId>;

    /// Delete a conversation by ID
    #[rpc(delete = "/conversation/{id}")]
    async fn delete_conversation(&self, id: &ConversationId) -> anyhow::Result<()>;

    /// Rename a conversation
    #[rpc(put = "/conversation/{id}")]
    async fn rename_conversation(&self, id: &ConversationId, name: &str) -> anyhow::Result<()>;
}
```

### Server side — service registration

```rust
// TraitName::service(arc) creates the dispatch service (struct name is hidden)
let dispatcher = Dispatcher::new()
    .register(<dyn McpApi>::service(daemon.clone()))
    .register(<dyn ModelApi>::service(daemon.clone()))
    .register(<dyn ConversationApi>::service(daemon.clone()));

// For stream-producing services, dispatch individually
let session_svc = <dyn SessionApi>::service(daemon.clone());
```

### Client side — one-liner trait impl

```rust
impl RpcClient for RemoteDaemon {
    type Stream = broadcast::Receiver<DaemonEvent>;
    async fn rpc_call(&self, method: &str, params: Value) -> Result<Value> { ... }
    async fn register_stream(&self, id: &str) -> Self::Stream { ... }
    async fn unregister_stream(&self, id: &str) { ... }
}

impl_remote_mcp_api!(RemoteDaemon);
impl_remote_session_api!(RemoteDaemon);
// ... one line per trait
```

---

## Annotations

| Annotation | Target | Effect |
|---|---|---|
| `#[rpc(skip)]` | Method | Not dispatched; client gets `bail!()` stub |
| `#[rpc(stream)]` | Method | Return type split: serializable part → RPC result, stream part → `DispatchResult.streams`. WebSocket only. |
| `#[rpc(base64_param = "name")]` | Method | Named `Vec<u8>` param encoded as base64 string over the wire |
| `#[rpc(base64_return)]` | Method | `Vec<u8>` return value encoded as base64 string over the wire |
| `#[rpc(get = "/path")]` | Method | Exposed as HTTP GET at the given path |
| `#[rpc(post = "/path")]` | Method | Exposed as HTTP POST at the given path |
| `#[rpc(put = "/path")]` | Method | Exposed as HTTP PUT at the given path |
| `#[rpc(delete = "/path")]` | Method | Exposed as HTTP DELETE at the given path |
| `#[rpc(no_tool)]` | Method | Exclude from generated `ToolDefinition` list. Combinable with REST annotations. |

### REST path templates

The path is written directly in the annotation, matching the URL it produces:

```rust
#[rpc(get    = "/conversation")]                        // GET /conversation
#[rpc(post   = "/conversation")]                        // POST /conversation
#[rpc(delete = "/conversation/{id}")]                   // DELETE /conversation/{id}
#[rpc(post   = "/session/{session_id}/message")]        // POST /session/{session_id}/message
```

**Rules:**
- `{name}` segments are matched to method parameters by name
- Remaining parameters (not in the path) come from the request body (POST/PUT) or query string (GET)
- Methods without a REST annotation are not exposed as REST endpoints
- The `#[rpc_service("prefix")]` on the trait is used for WebSocket dispatch and tool namespacing; REST paths are fully explicit per method

### Transport split

- **REST** — all methods with a REST annotation (`get`, `post`, `put`, `delete`). Request/response only.
- **WebSocket** — `#[rpc(stream)]` methods only. For event streams and bidirectional communication.
- **In-process only** — `#[rpc(skip)]` methods (e.g., `voice_connect` returning non-serializable handles).

A method is on exactly one transport. REST-annotated methods are not dispatched over WebSocket.

---

## Method classification

### Return types

| Pattern | Server dispatch | Client |
|---|---|---|
| `Result<()>` | `call_unit(...)` → `true` on success | `rpc_call(...)?; Ok(())` |
| `Result<T>` | `call_val(...)` → serialize T | `from_value(rpc_call(...)?)` |
| `T` (no Result) | `call_raw(...)` → serialize T | `rpc_call(...).unwrap_or_default()` |
| `#[rpc(stream)]` `Result<(T, S)>` | Serialize T, push S to `DispatchResult.streams` | `rpc_call` → deserialize T, `register_stream` → S |
| `#[rpc(stream)]` `Result<S>` | Push S to streams, return `true` | `rpc_call`, `register_stream` → S |

### Parameter patterns

| Pattern | Server (deserialize) | Client (serialize) |
|---|---|---|
| No params | No deserialization | `Value::Null` |
| Single `p: T` | `let p: T = from_value(params)?` | `to_value(&p)?` |
| Single `p: &T` | `let p: T = from_value(params)?` then `&p` | `to_value(p)?` |
| Single `p: &str` | `let p: String = from_value(params)?` then `&p` | `to_value(p)?` |
| Multiple params | Generate `struct Params { ... }`, deserialize | Generate `struct Params { ... }`, serialize |

---

## Architecture

### Core types

```rust
// Result of dispatching — carries RPC result + any streams produced
pub struct DispatchResult<S = ()> {
    pub result: RpcResult,
    pub streams: Vec<S>,
}

// Each service defines its own Stream type (no global enum)
pub trait RpcService: Send + Sync {
    type Stream: Send + 'static;
    fn prefix(&self) -> &str;
    fn meta(&self) -> &'static ServiceMeta;
    async fn dispatch(&self, method: &str, params: Value) -> Option<DispatchResult<Self::Stream>>;
}

// HashMap-based prefix routing for non-stream services
pub struct Dispatcher { ... }

// Client trait — implement for any network transport
pub trait RpcClient: Send + Sync {
    type Stream: Send + 'static;
    async fn rpc_call(&self, method: &str, params: Value) -> Result<Value>;
    async fn register_stream(&self, id: &str) -> Self::Stream;
    async fn unregister_stream(&self, id: &str);
}
```

### Per-service stream types

Each service has its own `Stream` associated type. `SessionApi` produces `broadcast::Receiver<DaemonEvent>`, a future `VideoApi` might produce something else entirely. No global enum needed — services are independently composable.

Stream-producing services are dispatched individually (typed). Non-stream services go through the `Dispatcher`.

### REST dispatch

Generated per-service from REST annotations:

```rust
pub struct RestMeta {
    pub http_method: HttpMethod,       // Get, Post, Put, Delete
    pub path_template: &'static str,   // e.g. "/session/{session_id}/message"
    pub method_name: &'static str,
}
```

`ServiceMeta` includes `rest_methods: &'static [RestMeta]`. Each service struct gets a `rest_dispatch(&self, http_method, path_segments, query, body) -> Option<Response>` method. The REST server iterates registered services and matches routes from metadata — no manual wiring.

### Tool generation

Each REST method (unless marked `no_tool`) generates a `ToolDefinition`:

- **Name:** `{prefix}_{method_name}` (e.g., `conversation_create_conversation`)
- **Description:** extracted from `///` doc comments on the trait method
- **Input schema:** auto-derived from method parameters via `schemars::JsonSchema`

For custom types that don't derive `JsonSchema` or need a different schema:

```rust
/// Override the auto-derived JSON Schema for a type.
pub trait RpcSchema {
    fn json_schema() -> schemars::schema::RootSchema;
}
```

The codegen checks: if a parameter type implements `RpcSchema`, use that; otherwise fall back to `schemars::schema_for!`. This lets you control the schema for opaque types (e.g., `SessionId` → `{"type": "string"}`).

The generated tools implement the `ToolService` trait — `get_definitions()` and `call_tool(name, args)`. This is a direct in-process trait impl, not an MCP server on a port. The daemon registers it alongside external MCP tools in `McpToolRegistry`, so agents see daemon capabilities and external MCP tools identically.

### Compatibility metadata

The macro generates `ServiceMeta` with per-method signature hashes. At connection time, the client sends its expected methods and the server checks compatibility: the server must be a superset with matching signatures.

```rust
pub struct ServiceMeta {
    pub prefix: &'static str,
    pub methods: &'static [MethodMeta],
    pub rest_methods: &'static [RestMeta],
}

pub fn check_compat(client: &[ServiceMetaWire], server: &[&ServiceMeta]) -> CompatResult;
```

### Client codegen

`impl_remote_xxx!` generates transport-appropriate client code:

- **REST methods** → HTTP calls via `reqwest`. Path template interpolation, JSON body serialization, response deserialization.
- **Stream methods** → WebSocket calls via `rpc_call` + `register_stream` (existing behavior).

`RemoteDaemon` holds both a base URL (for REST) and a lazy WebSocket connection (opened only when a stream method is called). Existing public API traits are unchanged — callers don't know the transport switched.

### WS server

The server is fully generic — takes a `DispatchFn` callback and knows nothing about specific APIs. Service wiring happens in `main.rs`:

```rust
// main.rs builds the dispatch function
fn build_dispatch(daemon: Arc<dyn DaemonApi>) -> ws::server::DispatchFn {
    let session_svc = <dyn SessionApi>::service(daemon.clone());
    // Only stream services need WebSocket dispatch now
    Arc::new(move |method, params, write_tx| { ... })
}
```

---

## Crate structure

```
simply-rpc/
  macros/
    src/
      lib.rs                 # #[rpc_service("prefix")] proc macro entry
      parse.rs               # Parse trait items, classify params/returns
      codegen_dispatch.rs    # Generate service struct + RpcService impl + REST dispatch + metadata
      codegen_client.rs      # Generate impl_remote_xxx! macro (HTTP for REST, WS for stream)
  src/
    lib.rs                   # Re-exports
    service.rs               # RpcService trait, Dispatcher
    client.rs                # RpcClient trait
    context.rs               # DispatchResult<S>
    helpers.rs               # call_unit, call_val, call_raw, encode/decode_base64
    meta.rs                  # ServiceMeta, MethodMeta, RestMeta, check_compat
    schema.rs                # RpcSchema trait for JSON Schema overrides

simply-daemon/
  src/
    remote.rs                # RemoteDaemon (public) — base URL + lazy WS, macro invocations
    ws/
      server.rs              # Generic WS server — stream methods only
      client.rs              # WsConnection (internal transport impl)
      rest.rs                # REST server — auto-routes from ServiceMeta
      discovery.rs           # connect_or_host() — takes build_dispatch callback
    api/
      session.rs             # #[rpc_service("session")] with #[rpc(stream)] + REST for non-streaming
      mcp.rs                 # #[rpc_service("mcp")] — all REST
      asset.rs               # #[rpc_service("asset")] — all REST
      daemon_info.rs         # #[rpc_service("daemon")] — health, kill, version
      ...                    # All traits annotated
    admin/
      mod.rs                 # Admin HTML page served at / (static route, not a trait)
    main.rs                  # Service wiring — build_dispatch(), REST server setup
```
