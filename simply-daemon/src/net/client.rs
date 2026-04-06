//! Daemon-specific RPC connection — implements `RpcConnection` over WebSocket.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use simply_rpc::ws_client::{ConnectionState, WsConnection, NotificationDemux};
use simply_rpc::RpcConnection;
use tokio::sync::{mpsc, watch};

use crate::api::types::DaemonEvent;
use crate::net::protocol::SessionEventParams;

/// Daemon RPC connection over WebSocket.
///
/// Implements `RpcConnection` for use by generated `RemoteXxxApi` structs.
pub struct DaemonRpcConnection {
    conn: WsConnection<DaemonEvent>,
    base_url: String,
    http: reqwest::Client,
}

impl DaemonRpcConnection {
    pub async fn connect(addr: &str, name: &str) -> anyhow::Result<Self> {
        let conn = WsConnection::connect(addr, daemon_demux()).await?;
        let _ = conn.rpc_call("client.identify", serde_json::json!({ "name": name })).await;

        Ok(Self {
            conn,
            base_url: format!("http://{addr}"),
            http: reqwest::Client::new(),
        })
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.conn.connection_state()
    }

    pub fn watch_state(&self) -> watch::Receiver<ConnectionState> {
        self.conn.watch_state()
    }
}

#[async_trait]
impl RpcConnection for DaemonRpcConnection {
    async fn rpc_call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.conn.rpc_call(method, params).await
    }

    async fn rest_call(
        &self,
        http_method: simply_rpc::HttpMethod,
        path: &str,
        body: Value,
    ) -> anyhow::Result<Value> {
        let url = format!("{}{}", self.base_url, path);

        let resp = match http_method {
            simply_rpc::HttpMethod::Get => self.http.get(&url).send().await?,
            simply_rpc::HttpMethod::Post => self.http.post(&url).json(&body).send().await?,
            simply_rpc::HttpMethod::Put => self.http.put(&url).json(&body).send().await?,
            simply_rpc::HttpMethod::Delete => self.http.delete(&url).send().await?,
        };

        let status = resp.status();
        if status.is_success() {
            let text = resp.text().await?;
            if text.is_empty() {
                Ok(Value::Null)
            } else {
                Ok(serde_json::from_str(&text)?)
            }
        } else {
            let msg = resp.text().await.unwrap_or_else(|_| status.to_string());
            Err(anyhow::anyhow!("REST error {}: {}", status, msg))
        }
    }

    async fn register_stream(&self, id: &str) -> mpsc::Receiver<Value> {
        // Use the broadcast stream from WsConnection, convert to mpsc<Value>
        let mut broadcast_rx = self.conn.register_stream(id).await;
        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            loop {
                match broadcast_rx.recv().await {
                    Ok(event) => {
                        let value = serde_json::to_value(&event).unwrap_or_default();
                        if tx.send(value).await.is_err() { break; }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        rx
    }

    async fn unregister_stream(&self, id: &str) {
        self.conn.unregister_stream(id).await;
    }

    async fn register_bidi_stream_raw(
        &self,
        method: &str,
        _path: &str,
    ) -> anyhow::Result<(mpsc::Sender<Value>, mpsc::Receiver<Value>)> {
        let event_method = format!("{method}.event");
        let input_method = format!("{method}.input");

        // Subscribe to events from the server
        let mut raw_rx = self.conn.register_raw_sink(&event_method).await;

        // Get a write channel for sending notifications
        let write_tx = self.conn.write_tx().await
            .ok_or_else(|| anyhow::anyhow!("disconnected"))?;

        // Input: caller sends Value → we serialize as WS notification
        let (input_tx, mut input_rx) = mpsc::channel::<Value>(64);
        tokio::spawn(async move {
            while let Some(value) = input_rx.recv().await {
                let notif = simply_rpc::protocol::WsNotification {
                    method: input_method.clone(),
                    params: value,
                };
                let text = serde_json::to_string(&notif).unwrap_or_default();
                if write_tx.send(text).await.is_err() { break; }
            }
        });

        // Output: raw events from the server
        let (output_tx, output_rx) = mpsc::channel::<Value>(64);
        tokio::spawn(async move {
            while let Some(value) = raw_rx.recv().await {
                if output_tx.send(value).await.is_err() { break; }
            }
        });

        Ok((input_tx, output_rx))
    }
}

/// Create a daemon-specific notification demuxer.
fn daemon_demux() -> NotificationDemux<DaemonEvent> {
    Arc::new(|method, params| {
        if method == "session.event" {
            if let Ok(p) = serde_json::from_value::<SessionEventParams>(params) {
                return Some((p.session_id.as_str().to_string(), p.event));
            }
        }
        None
    })
}
