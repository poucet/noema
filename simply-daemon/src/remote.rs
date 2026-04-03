//! RemoteDaemon — implements `DaemonApi` over REST + WebSocket.
//!
//! REST for request/response methods, WebSocket for streaming.
//! This is the client-side counterpart to the daemon server.

use std::sync::Arc;

use async_trait::async_trait;
use simply_rpc::{HttpMethod, RpcClient};
use tokio::sync::{broadcast, watch};

use crate::api::*;
use crate::ws::client::WsConnection;
use crate::ws::ConnectionState;

/// A daemon client that talks to a remote daemon over REST + WebSocket.
///
/// Implements all `DaemonApi` traits via generated dispatch.
/// REST methods use HTTP (reqwest), stream methods use WebSocket.
/// Reconnects automatically with exponential backoff when the daemon restarts.
pub struct RemoteDaemon {
    conn: WsConnection,
    http: reqwest::Client,
    base_url: String,
}

impl RemoteDaemon {
    /// Connect to a running daemon at the given address.
    pub async fn connect(addr: &str) -> anyhow::Result<Arc<Self>> {
        Self::connect_as(addr, "unknown").await
    }

    /// Connect and identify with a client name (shown in admin dashboard).
    pub async fn connect_as(addr: &str, name: &str) -> anyhow::Result<Arc<Self>> {
        let conn = WsConnection::connect(addr).await?;
        // Identify to the server (best-effort, don't fail on error)
        let _ = conn.rpc_call("client.identify", serde_json::json!({ "name": name })).await;

        // Derive REST base URL from WebSocket address
        // WS addr is like "ws://127.0.0.1:9800" or "127.0.0.1:9800"
        // REST is on port+1
        let base_url = derive_rest_url(addr);

        Ok(Arc::new(Self {
            conn,
            http: reqwest::Client::new(),
            base_url,
        }))
    }

    /// Convert to a trait object. Use this when you need `Arc<dyn DaemonApi>`.
    pub fn into_daemon(self: Arc<Self>) -> Arc<dyn DaemonApi> {
        self
    }

    /// Current connection state.
    pub fn connection_state(&self) -> ConnectionState {
        self.conn.connection_state()
    }

    /// Watch connection state changes (for UI status indicators).
    pub fn watch_connection_state(&self) -> watch::Receiver<ConnectionState> {
        self.conn.watch_state()
    }
}

/// Derive the REST base URL from the WebSocket address.
/// WS: "ws://127.0.0.1:9800" or "127.0.0.1:9800" → REST: "http://127.0.0.1:9801"
fn derive_rest_url(ws_addr: &str) -> String {
    let addr = ws_addr
        .trim_start_matches("ws://")
        .trim_start_matches("wss://");
    if let Some((host, port_str)) = addr.rsplit_once(':') {
        if let Ok(port) = port_str.parse::<u16>() {
            return format!("http://{}:{}", host, port + 1);
        }
    }
    format!("http://{addr}")
}

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

    async fn rest_call(
        &self,
        http_method: HttpMethod,
        path: &str,
        body: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!("{}{}", self.base_url, path);

        let resp = match http_method {
            HttpMethod::Get => self.http.get(&url).send().await?,
            HttpMethod::Post => self.http.post(&url).json(&body).send().await?,
            HttpMethod::Put => self.http.put(&url).json(&body).send().await?,
            HttpMethod::Delete => self.http.delete(&url).send().await?,
        };

        let status = resp.status();
        if status.is_success() {
            let text = resp.text().await?;
            if text.is_empty() {
                Ok(serde_json::Value::Null)
            } else {
                Ok(serde_json::from_str(&text)?)
            }
        } else {
            let msg = resp.text().await.unwrap_or_else(|_| status.to_string());
            Err(anyhow::anyhow!("REST error {}: {}", status, msg))
        }
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
