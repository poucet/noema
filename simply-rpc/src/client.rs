use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

/// Trait-object-safe RPC connection.
///
/// The core transport interface. Implementations handle the network protocol
/// (WS, HTTP, in-process). Generated `RemoteXxxApi` structs use this to
/// communicate with the server.
///
/// All methods work with raw `serde_json::Value` — the generated code
/// handles typed serialization/deserialization.
#[async_trait]
pub trait RpcConnection: Send + Sync {
    /// Send an RPC request and wait for the JSON response.
    async fn rpc_call(
        &self,
        method: &str,
        params: Value,
    ) -> anyhow::Result<Value>;

    /// Make a REST HTTP call.
    async fn rest_call(
        &self,
        http_method: crate::HttpMethod,
        path: &str,
        body: Value,
    ) -> anyhow::Result<Value>;

    /// Register a stream for the given ID. Returns raw JSON events.
    async fn register_stream(&self, id: &str) -> mpsc::Receiver<Value>;

    /// Unregister a stream.
    async fn unregister_stream(&self, id: &str);

    /// Register a bidirectional stream (raw JSON channels).
    ///
    /// Returns `(input_sink, output_stream)`:
    /// - Send `Value` to `input_sink` → serialized and sent as `{method}.input` notifications
    /// - Receive `Value` from `output_stream` ← deserialized from `{method}.event` notifications
    async fn register_bidi_stream_raw(
        &self,
        method: &str,
        path: &str,
    ) -> anyhow::Result<(mpsc::Sender<Value>, mpsc::Receiver<Value>)>;
}
