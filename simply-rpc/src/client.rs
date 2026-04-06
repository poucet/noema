use async_trait::async_trait;
use serde_json::Value;

/// Trait for types that can make RPC calls over a network.
///
/// Implement this on your remote client (e.g. `RemoteDaemon`), then use
/// the generated `impl_remote_xxx!` macros to get trait impls for free.
#[async_trait]
pub trait RpcClient: Send + Sync {
    /// The stream type returned by `#[rpc(stream)]` methods.
    /// Use `()` if no stream methods are needed.
    type Stream: Send + 'static;

    /// Send an RPC request and wait for the response.
    async fn rpc_call(
        &self,
        method: &str,
        params: Value,
    ) -> anyhow::Result<Value>;

    /// Register a stream for the given ID.
    /// Called by generated client code for `#[rpc(stream)]` methods.
    async fn register_stream(&self, id: &str) -> Self::Stream;

    /// Unregister a stream. Called on cleanup (e.g. close_session).
    async fn unregister_stream(&self, id: &str);

    /// Make a REST HTTP call. Generated client code calls this for REST-annotated methods.
    async fn rest_call(
        &self,
        http_method: crate::HttpMethod,
        path: &str,
        body: Value,
    ) -> anyhow::Result<Value>;

    /// Register a bidirectional stream for a given method.
    ///
    /// The RPC call has already been made (setting up the server side).
    /// This method establishes the client-side channel bridging: incoming
    /// WS frames are deserialized as `U` and sent to the returned handle's
    /// receiver, while messages sent through the handle's sender are
    /// serialized and sent as WS frames.
    ///
    /// Default implementation returns an error — override in clients that
    /// support bidirectional streaming.
    async fn register_bidi_stream<T, U>(
        &self,
        _method: &str,
    ) -> anyhow::Result<crate::StreamHandle<T, U>>
    where
        T: serde::Serialize + Send + 'static,
        U: serde::de::DeserializeOwned + Send + 'static,
    {
        anyhow::bail!("bidirectional streams not supported by this client")
    }
}
