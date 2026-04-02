# simply-rpc — Generic Trait-over-Network RPC Framework

**Status:** Implemented
**Crate:** `simply-rpc/` (+ `simply-rpc/macros/`)

---

## Overview

A proc macro `#[rpc_service("prefix")]` annotates async traits and auto-generates:
1. A **server service struct** implementing `RpcService` — dispatches JSON-RPC calls to trait methods
2. A **client impl macro** — implements the trait for any type implementing `RpcClient`

The framework is **generic** — it knows nothing about daemons, sessions, or LLMs. The daemon is just one consumer.

---

## Usage

### Annotating a trait

```rust
#[rpc_service("mcp")]
#[async_trait]
pub trait McpApi: Send + Sync {
    async fn list_mcp_servers(&self) -> anyhow::Result<Vec<McpServerInfo>>;
    async fn connect_mcp_server(&self, server_id: &str) -> anyhow::Result<usize>;
    async fn remove_mcp_server(&self, server_id: &str) -> anyhow::Result<()>;
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
| `#[rpc(stream)]` | Method | Return type split: serializable part → RPC result, stream part → `DispatchResult.streams` |
| `#[rpc(base64_param = "name")]` | Method | Named `Vec<u8>` param encoded as base64 string over the wire |
| `#[rpc(base64_return)]` | Method | `Vec<u8>` return value encoded as base64 string over the wire |

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

### Compatibility metadata

The macro generates `ServiceMeta` with per-method signature hashes. At connection time, the client sends its expected methods and the server checks compatibility: the server must be a superset with matching signatures.

```rust
pub struct ServiceMeta {
    pub prefix: &'static str,
    pub methods: &'static [MethodMeta],
}

pub fn check_compat(client: &[ServiceMetaWire], server: &[&ServiceMeta]) -> CompatResult;
```

### WS server

The server is fully generic — takes a `DispatchFn` callback and knows nothing about specific APIs. Service wiring happens in `main.rs`:

```rust
// main.rs builds the dispatch function
fn build_dispatch(daemon: Arc<dyn DaemonApi>) -> ws::server::DispatchFn {
    let session_svc = <dyn SessionApi>::service(daemon.clone());
    let dispatcher = Dispatcher::new()
        .register(<dyn McpApi>::service(daemon.clone()))
        // ...
    ;
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
      codegen_dispatch.rs    # Generate service struct + RpcService impl + metadata
      codegen_client.rs      # Generate impl_remote_xxx! macro
  src/
    lib.rs                   # Re-exports
    service.rs               # RpcService trait, Dispatcher
    client.rs                # RpcClient trait
    context.rs               # DispatchResult<S>
    helpers.rs               # call_unit, call_val, call_raw, encode/decode_base64
    meta.rs                  # ServiceMeta, MethodMeta, check_compat

simply-daemon/
  src/
    remote.rs                # RemoteDaemon (public) — 7 one-liner macro invocations
    ws/
      server.rs              # Generic WS server — takes DispatchFn
      client.rs              # WsConnection (internal transport impl)
      discovery.rs           # connect_or_host() — takes build_dispatch callback
    api/
      session.rs             # #[rpc_service("session")] with #[rpc(stream)]
      mcp.rs                 # #[rpc_service("mcp")]
      asset.rs               # #[rpc_service("asset")] with #[rpc(base64_param/return)]
      ...                    # All 7 traits annotated
    main.rs                  # Service wiring — build_dispatch()
```

---

## Future

- `#[rpc(rest)]` — expose methods as REST GET endpoints with HTTP caching
- Connection identity + reconnection (task 2.9)
- Voice/video channels via `#[rpc(channel)]` for bidirectional binary streams
