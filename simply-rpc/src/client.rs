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

    /// Register a bidirectional stream.
    ///
    /// The RPC call has already been made (setting up the server side).
    /// `method` is the RPC method name (e.g. "voice.voice_connect").
    /// `path` is the stream path (e.g. "/voice/stream/whisper").
    ///
    /// The client sends `T` as JSON notifications on the shared WS
    /// (method: "{method}.input"), and receives `U` events
    /// (method: "{method}.event").
    ///
    /// Default implementation returns an error — override in clients that
    /// support bidirectional streaming.
    async fn register_bidi_stream<T, U>(
        &self,
        _method: &str,
        _path: &str,
    ) -> anyhow::Result<crate::StreamHandle<T, U>>
    where
        T: serde::Serialize + Send + 'static,
        U: serde::de::DeserializeOwned + Send + 'static,
    {
        anyhow::bail!("bidirectional streams not supported by this client")
    }
}
