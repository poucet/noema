# daemon-rpc Macro System Design

**Goal:** A proc macro `#[daemon_rpc("prefix")]` that annotates DaemonApi traits and auto-generates the WS server dispatch + remote client implementation. Adding a new API trait should require only the annotation and adding it to the server/client trait lists.

---

## What the macro generates

Given:
```rust
#[daemon_rpc("session")]
#[async_trait]
pub trait SessionApi: Send + Sync {
    async fn close_session(&self, session_id: &SessionId) -> anyhow::Result<()>;
    async fn get_messages(&self, session_id: &SessionId) -> anyhow::Result<Vec<ResolvedMessage>>;
    async fn set_model(&self, session_id: &SessionId, model_id: &str) -> anyhow::Result<()>;
    async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>>;

    #[rpc(skip)]  // not serializable over WS
    async fn create_session(&self, options: CreateSessionOptions)
        -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)>;
}
```

It generates two items alongside the trait (trait itself passes through unchanged):

### 1. Server dispatch function

```rust
pub async fn dispatch_session_api(
    daemon: &(dyn SessionApi + Send + Sync),
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> Option<daemon_rpc::RpcResult> {
    match method {
        "session.close_session" => {
            // Deserialize: &SessionId → owned SessionId
            let session_id: SessionId = daemon_rpc::de(params)?;
            let r = daemon.close_session(&session_id).await;
            Some(daemon_rpc::into_unit_response(r))
        }
        "session.get_messages" => {
            let session_id: SessionId = daemon_rpc::de(params)?;
            let r = daemon.get_messages(&session_id).await;
            Some(daemon_rpc::into_response(r))
        }
        "session.set_model" => {
            #[derive(serde::Deserialize)]
            struct Params { session_id: SessionId, model_id: String }
            let p: Params = daemon_rpc::de(params)?;
            let r = daemon.set_model(&p.session_id, &p.model_id).await;
            Some(daemon_rpc::into_unit_response(r))
        }
        "session.list_sessions" => {
            let r = daemon.list_sessions().await;
            Some(daemon_rpc::into_response(r))
        }
        _ => None,  // not our method
    }
}
```

### 2. Client impl block

```rust
// Generated as a declarative macro so the user controls which type it applies to
macro_rules! impl_remote_session_api {
    ($T:ty) => {
        #[async_trait::async_trait]
        impl SessionApi for $T {
            async fn close_session(&self, session_id: &SessionId) -> anyhow::Result<()> {
                self.rpc_call("session.close_session", session_id).await?;
                Ok(())
            }
            async fn get_messages(&self, session_id: &SessionId) -> anyhow::Result<Vec<ResolvedMessage>> {
                let r = self.rpc_call("session.get_messages", session_id).await?;
                Ok(serde_json::from_value(r)?)
            }
            async fn set_model(&self, session_id: &SessionId, model_id: &str) -> anyhow::Result<()> {
                #[derive(serde::Serialize)]
                struct Params<'a> { session_id: &'a SessionId, model_id: &'a str }
                self.rpc_call("session.set_model", &Params { session_id, model_id }).await?;
                Ok(())
            }
            async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>> {
                let r = self.rpc_call("session.list_sessions", &()).await?;
                Ok(serde_json::from_value(r)?)
            }

            // Skipped methods get a stub that returns an error
            async fn create_session(&self, _options: CreateSessionOptions)
                -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)> {
                anyhow::bail!("create_session requires manual implementation for stream handling")
            }
        }
    };
}
```

---

## Method classification

The macro inspects each method signature and classifies it:

| Pattern | Server behavior | Client behavior |
|---------|----------------|-----------------|
| `-> anyhow::Result<()>` | Call, return `true` on ok | Call, ignore result value |
| `-> anyhow::Result<T>` | Call, serialize `T` | Call, deserialize `T` |
| `-> T` (no Result) | Call, serialize `T` | Call, deserialize `T` |
| `#[rpc(skip)]` | Not included in dispatch | Stub returning error |

### Parameter handling

| Param pattern | Server (deserialize) | Client (serialize) |
|--------------|---------------------|-------------------|
| No params (just `&self`) | No deserialization | Serialize `()` |
| Single owned: `foo: Foo` | `let foo: Foo = de(params)` | Serialize `&foo` |
| Single ref: `foo: &Foo` | `let foo: Foo = de(params)` then `&foo` | Serialize `foo` (already a ref) |
| Single `&str` | `let foo: String = de(params)` then `&foo` | Serialize `foo` |
| Multiple params | Generate `struct Params { ... }` with all owned types, deserialize as struct | Generate `struct Params<'a> { ... }` with ref types, serialize as struct |

---

## Helper types (in `daemon-rpc` crate or a shared module)

The generated code references helper functions for serialization. These live in a small runtime module (not the proc macro crate itself):

```rust
// daemon_rpc_runtime or simply_daemon::ws::rpc_helpers
pub type RpcResult = Result<serde_json::Value, String>;

pub fn de<T: serde::de::DeserializeOwned>(params: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(params).map_err(|e| e.to_string())
}

pub fn into_response<T: serde::Serialize>(r: anyhow::Result<T>) -> RpcResult {
    match r {
        Ok(v) => Ok(serde_json::to_value(v).unwrap_or_default()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn into_unit_response(r: anyhow::Result<()>) -> RpcResult {
    match r {
        Ok(()) => Ok(serde_json::Value::Bool(true)),
        Err(e) => Err(e.to_string()),
    }
}

pub fn into_raw_response<T: serde::Serialize>(v: T) -> RpcResult {
    Ok(serde_json::to_value(v).unwrap_or_default())
}
```

The dispatch function returns `Option<RpcResult>` — `None` means "not my method", `Some(Ok(value))` is success, `Some(Err(msg))` is error. The WS server converts this to `WsResponse`.

---

## Server wiring

```rust
// ws/server.rs — dispatch function
async fn dispatch(daemon: &dyn DaemonApi, id: u64, method: &str, params: Value, ...) -> WsResponse {
    // Try each trait's dispatch in order
    if let Some(r) = dispatch_session_api(daemon, id, method, params.clone()).await {
        return to_ws_response(id, r);
    }
    if let Some(r) = dispatch_conversation_api(daemon, id, method, params.clone()).await {
        return to_ws_response(id, r);
    }
    if let Some(r) = dispatch_mcp_api(daemon, id, method, params.clone()).await {
        return to_ws_response(id, r);
    }
    // ... etc
    WsResponse::err(id, format!("unknown method: {method}"))
}
```

Adding a new trait = add one `if let Some` line.

---

## Client wiring

```rust
// ws/client.rs
pub struct RemoteDaemon { ... }

impl RemoteDaemon {
    /// Core RPC call — serialize, send, wait for response, return value.
    pub async fn rpc_call(&self, method: &str, params: &impl Serialize) -> anyhow::Result<Value> { ... }

    /// Register a session broadcast channel (for stream methods).
    pub async fn register_session(&self, id: &SessionId) -> broadcast::Receiver<DaemonEvent> { ... }
}

// Generated by macro:
impl_remote_session_api!(RemoteDaemon);
impl_remote_conversation_api!(RemoteDaemon);
impl_remote_mcp_api!(RemoteDaemon);
impl_remote_oauth_api!(RemoteDaemon);
impl_remote_model_api!(RemoteDaemon);

// Manual impls for special methods:
impl SessionApi for RemoteDaemon {
    // create_session, resume_session, subscribe_session — manual
    // because they involve broadcast channel registration
}
// But wait — the macro stub conflicts with a manual impl of the same trait.
// Solution: don't skip these in the macro. Instead, the macro handles them with
// a #[rpc(session_create)] or #[rpc(session_subscribe)] attribute that generates
// the correct client-side code (call + register_session).
```

### Handling session stream methods in the macro

Instead of `#[rpc(skip)]`, use specific attributes:

- `#[rpc(session_create)]` — client calls RPC, deserializes SessionInfo, calls `self.register_session(&info.id)`, returns `(info, rx)`
- `#[rpc(session_subscribe)]` — client calls RPC, calls `self.register_session(session_id)`, returns `rx`

Server side for these: generated dispatch calls the method, then **the server.rs wrapper** handles spawning the event forwarder (not the generated code). The dispatch function returns a special `RpcResult` variant or the server intercepts methods starting with `"session.create"` etc.

Actually simpler: the dispatch function for session stream methods returns the `SessionInfo` as the result. The server's per-connection handler knows that `"session.create_session"` and `"session.resume_session"` need event forwarder spawning — this is wired once in the server, not generated.

---

## Skipped traits

- `VoiceApi` — bidirectional audio streaming, not RPC-able. Manual `#[rpc(skip)]` on entire trait or just don't annotate it.
- `AssetApi` — `Vec<u8>` needs base64 encoding. Use `#[rpc(base64)]` on params/returns, or handle manually. For now, keep manual.

---

## Crate structure

```
simply-daemon/
  macros/                    # proc macro crate: "daemon-rpc"
    Cargo.toml
    src/lib.rs               # #[daemon_rpc("prefix")] proc macro
    tests/                   # compile-time tests with dummy traits
      basic.rs               # simple trait, verify codegen
      multi_param.rs         # multi-param methods
      refs.rs                # &str, &SessionId params
      no_result.rs           # methods returning T instead of Result<T>
  src/
    ws/
      rpc.rs                 # runtime helpers (de, into_response, etc.)
      server.rs              # uses dispatch_*_api() functions
      client.rs              # uses impl_remote_*_api!() macros + manual stream impls
```

---

## Type assumptions

For all parameters:
- Must implement `serde::Deserialize` (server side)
- Must implement `serde::Serialize` (client side)
- `&str` → deserialized as `String`, passed as `&str`
- `&T` → deserialized as `T`, passed as `&T`

For return types:
- `T` in `Result<T>` must implement `serde::Serialize` (server) and `serde::Deserialize` (client)

---

## Open questions

1. **`#[async_trait]` ordering** — The proc macro must run before `async_trait` transforms the trait. Attribute order matters: `#[daemon_rpc("session")]` must come before `#[async_trait]`. The macro should emit the trait with `#[async_trait]` still on it so async_trait processes it after.

2. **Macro hygiene for the generated `struct Params`** — Use `__DaemonRpcParams_{method_name}` or similar unique names to avoid collisions.

3. **The `impl_remote_*!` macro and trait coherence** — If session stream methods are included in the generated impl, they need the correct return type. The macro needs to understand `broadcast::Receiver<DaemonEvent>` in the return type to generate `self.register_session()` calls.
