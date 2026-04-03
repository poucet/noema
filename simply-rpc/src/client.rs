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
    /// Default implementation falls back to `rpc_call` (for backward compat).
    async fn rest_call(
        &self,
        http_method: crate::HttpMethod,
        path: &str,
        body: Value,
    ) -> anyhow::Result<Value> {
        // Default: fall back to RPC (backward compat for clients that haven't upgraded)
        let method_name = format!("__rest.{}.{}", match http_method {
            crate::HttpMethod::Get => "GET",
            crate::HttpMethod::Post => "POST",
            crate::HttpMethod::Put => "PUT",
            crate::HttpMethod::Delete => "DELETE",
            crate::HttpMethod::Stream => "STREAM",
        }, path);
        self.rpc_call(&method_name, body).await
    }
}
