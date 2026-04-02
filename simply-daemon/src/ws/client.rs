//! WebSocket RPC client — transport implementation for RemoteDaemon.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use simply_rpc::RpcClient;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::api::*;
use super::protocol::*;

/// Internal WS connection state. RemoteDaemon delegates to this.
pub(crate) struct WsConnection {
    write_tx: mpsc::Sender<String>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>>,
    session_senders: Arc<Mutex<HashMap<SessionId, broadcast::Sender<DaemonEvent>>>>,
    next_id: AtomicU64,
    _reader: JoinHandle<()>,
    _writer: JoinHandle<()>,
}

impl WsConnection {
    /// Connect to a daemon at the given address.
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        let url = format!("ws://{}", addr);
        let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await?;
        let (ws_sink, ws_source) = ws_stream.split();

        let (write_tx, mut write_rx) = mpsc::channel::<String>(256);
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let session_senders: Arc<Mutex<HashMap<SessionId, broadcast::Sender<DaemonEvent>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let writer = tokio::spawn(async move {
            let mut sink = ws_sink;
            while let Some(text) = write_rx.recv().await {
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        });

        let pending_clone = Arc::clone(&pending);
        let senders_clone = Arc::clone(&session_senders);
        let reader = tokio::spawn(async move {
            let mut source = ws_source;
            while let Some(msg) = source.next().await {
                let text = match msg {
                    Ok(Message::Text(t)) => t.to_string(),
                    Ok(Message::Close(_)) => break,
                    Ok(_) => continue,
                    Err(_) => break,
                };

                let incoming: WsIncoming = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if incoming.is_response() {
                    let id = incoming.id.unwrap();
                    let response = WsResponse {
                        id,
                        result: incoming.result,
                        error: incoming.error,
                    };
                    if let Some(tx) = pending_clone.lock().await.remove(&id) {
                        let _ = tx.send(response);
                    }
                } else if incoming.is_notification() {
                    let method = incoming.method.as_deref().unwrap();
                    if method == "session.event" {
                        if let Ok(params) = serde_json::from_value::<SessionEventParams>(incoming.params) {
                            let senders = senders_clone.lock().await;
                            if let Some(tx) = senders.get(&params.session_id) {
                                let _ = tx.send(params.event);
                            }
                        }
                    }
                }
            }

            // Connection lost — fail all pending requests
            let mut pending = pending_clone.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(WsResponse::err(0, "connection lost"));
            }
        });

        Ok(Self {
            write_tx,
            pending,
            session_senders,
            next_id: AtomicU64::new(1),
            _reader: reader,
            _writer: writer,
        })
    }
}

#[async_trait]
impl RpcClient for WsConnection {
    type Stream = broadcast::Receiver<DaemonEvent>;

    async fn rpc_call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let request = WsRequest {
            id,
            method: method.to_string(),
            params,
        };

        tracing::debug!(id, method, "WS client request");

        self.write_tx
            .send(serde_json::to_string(&request)?)
            .await
            .map_err(|_| anyhow::anyhow!("connection closed"))?;

        let response = rx.await.map_err(|_| anyhow::anyhow!("connection closed"))?;

        let is_err = response.error.is_some();
        tracing::debug!(id, method, error = is_err, "WS client response");

        match response.error {
            Some(e) => Err(anyhow::anyhow!(e.message)),
            None => Ok(response.result.unwrap_or(serde_json::Value::Null)),
        }
    }

    async fn register_stream(&self, id: &str) -> Self::Stream {
        let sid = SessionId::new(id);
        let mut senders = self.session_senders.lock().await;
        if let Some(tx) = senders.get(&sid) {
            return tx.subscribe();
        }
        let (tx, rx) = broadcast::channel(256);
        senders.insert(sid, tx);
        rx
    }

    async fn unregister_stream(&self, id: &str) {
        let sid = SessionId::new(id);
        self.session_senders.lock().await.remove(&sid);
    }
}
