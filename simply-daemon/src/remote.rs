//! RemoteDaemon — implements `DaemonApi` over REST + WebSocket.
//!
//! REST for request/response methods, WebSocket for streaming.

use std::sync::Arc;

use async_trait::async_trait;
use simply_rpc::{HttpMethod, RpcClient};
use simply_rpc::ws_client::ConnectionState;
use tokio::sync::{broadcast, watch};

use crate::api::*;
use crate::net::client::{DaemonWsConnection, daemon_demux};

/// A daemon client that talks to a remote daemon over REST + WebSocket.
pub struct RemoteDaemon {
    conn: DaemonWsConnection,
    http: reqwest::Client,
    base_url: String,
}

impl RemoteDaemon {
    pub async fn connect(addr: &str) -> anyhow::Result<Arc<Self>> {
        Self::connect_as(addr, "unknown").await
    }

    pub async fn connect_as(addr: &str, name: &str) -> anyhow::Result<Arc<Self>> {
        let conn = DaemonWsConnection::connect(addr, daemon_demux()).await?;
        let _ = conn.rpc_call("client.identify", serde_json::json!({ "name": name })).await;

        let base_url = format!("http://{addr}");

        Ok(Arc::new(Self {
            conn,
            http: reqwest::Client::new(),
            base_url,
        }))
    }

    pub fn into_daemon(self: Arc<Self>) -> Arc<dyn DaemonApi> {
        self
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.conn.connection_state()
    }

    pub fn watch_connection_state(&self) -> watch::Receiver<ConnectionState> {
        self.conn.watch_state()
    }
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

impl_remote_session_api!(RemoteDaemon);
impl_remote_conversation_api!(RemoteDaemon);
impl_remote_asset_api!(RemoteDaemon);
impl_remote_mcp_api!(RemoteDaemon);
impl_remote_o_auth_api!(RemoteDaemon);
impl_remote_model_api!(RemoteDaemon);
impl_remote_voice_api!(RemoteDaemon);
