//! RemoteDaemon — implements `DaemonApi` over a WebSocket connection.
//!
//! This is the client-side counterpart to the WS server. Any process that
//! connects to a running daemon gets an `Arc<RemoteDaemon>` which implements
//! all the API traits via generated RPC calls.

use std::sync::Arc;

use async_trait::async_trait;
use simply_rpc::RpcClient;
use tokio::sync::broadcast;

use crate::api::*;
use crate::ws::client::WsConnection;

/// A daemon client that talks to a remote daemon over WebSocket.
///
/// Implements all `DaemonApi` traits via generated RPC dispatch.
/// Construct with `RemoteDaemon::connect(addr)`.
pub struct RemoteDaemon {
    conn: WsConnection,
}

impl RemoteDaemon {
    /// Connect to a running daemon at the given address.
    pub async fn connect(addr: &str) -> anyhow::Result<Arc<Self>> {
        let conn = WsConnection::connect(addr).await?;
        Ok(Arc::new(Self { conn }))
    }

    /// Convert to a trait object. Use this when you need `Arc<dyn DaemonApi>`.
    pub fn into_daemon(self: Arc<Self>) -> Arc<dyn DaemonApi> {
        self
    }
}

// Delegate RpcClient to the inner WsConnection
#[async_trait]
impl RpcClient for RemoteDaemon {
    type Stream = broadcast::Receiver<DaemonEvent>;

    async fn rpc_call(&self, method: &str, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        self.conn.rpc_call(method, params).await
    }

    async fn register_stream(&self, id: &str) -> Self::Stream {
        self.conn.register_stream(id).await
    }

    async fn unregister_stream(&self, id: &str) {
        self.conn.unregister_stream(id).await
    }
}

// ---------------------------------------------------------------------------
// Generated trait implementations — one line each
// ---------------------------------------------------------------------------

impl_remote_session_api!(RemoteDaemon);
impl_remote_conversation_api!(RemoteDaemon);
impl_remote_asset_api!(RemoteDaemon);
impl_remote_mcp_api!(RemoteDaemon);
impl_remote_o_auth_api!(RemoteDaemon);
impl_remote_model_api!(RemoteDaemon);
impl_remote_voice_api!(RemoteDaemon);
