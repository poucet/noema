//! WebSocket RPC client — transport implementation for RemoteDaemon.
//!
//! Automatically reconnects with exponential backoff when the connection drops.
//! The daemon can restart independently; clients reconnect transparently.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use simply_rpc::RpcClient;
use tokio::sync::{broadcast, mpsc, oneshot, watch, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::api::*;
use super::protocol::*;

/// Observable connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Reconnecting,
}

/// Backoff configuration.
const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const BACKOFF_MULTIPLIER: u32 = 2;

/// Active connection — reader + writer tasks and the write channel.
struct LiveConnection {
    write_tx: mpsc::Sender<String>,
    _reader: JoinHandle<()>,
    _writer: JoinHandle<()>,
}

/// Internal WS connection state. RemoteDaemon delegates to this.
pub(crate) struct WsConnection {
    live: Arc<Mutex<Option<LiveConnection>>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>>,
    session_senders: Arc<Mutex<HashMap<SessionId, broadcast::Sender<DaemonEvent>>>>,
    next_id: AtomicU64,
    state_rx: watch::Receiver<ConnectionState>,
    _reconnect_task: JoinHandle<()>,
}

impl WsConnection {
    /// Connect to a daemon at the given address. Reconnects automatically on disconnect.
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let session_senders: Arc<Mutex<HashMap<SessionId, broadcast::Sender<DaemonEvent>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let live: Arc<Mutex<Option<LiveConnection>>> = Arc::new(Mutex::new(None));

        // Initial connect — fail if we can't reach the daemon at all.
        // Initialize watch as Connected so the reconnect task doesn't race.
        let (state_tx, state_rx) = watch::channel(ConnectionState::Connected);
        let initial = establish_connection(addr, Arc::clone(&pending), Arc::clone(&session_senders), Arc::clone(&live), state_tx.clone()).await?;
        *live.lock().await = Some(initial);

        // Spawn reconnect task
        let reconnect_task = {
            let addr = addr.to_string();
            let live = Arc::clone(&live);
            let pending = Arc::clone(&pending);
            let session_senders = Arc::clone(&session_senders);
            let state_tx = state_tx.clone();
            let mut state_rx = state_rx.clone();

            tokio::spawn(async move {
                loop {
                    // Wait until we're disconnected
                    loop {
                        if *state_rx.borrow_and_update() == ConnectionState::Disconnected {
                            break;
                        }
                        if state_rx.changed().await.is_err() {
                            return; // sender dropped, shutting down
                        }
                    }

                    let _ = state_tx.send(ConnectionState::Reconnecting);
                    tracing::info!(addr = %addr, "Connection lost, starting reconnect");

                    let mut backoff = INITIAL_BACKOFF;
                    loop {
                        tokio::time::sleep(backoff).await;

                        match establish_connection(&addr, Arc::clone(&pending), Arc::clone(&session_senders), Arc::clone(&live), state_tx.clone()).await {
                            Ok(conn) => {
                                *live.lock().await = Some(conn);
                                let _ = state_tx.send(ConnectionState::Connected);
                                tracing::info!(addr = %addr, "Reconnected to daemon");
                                break;
                            }
                            Err(e) => {
                                tracing::debug!(addr = %addr, backoff_ms = backoff.as_millis(), error = %e, "Reconnect failed, retrying");
                                backoff = (backoff * BACKOFF_MULTIPLIER).min(MAX_BACKOFF);
                            }
                        }
                    }
                }
            })
        };

        Ok(Self {
            live,
            pending,
            session_senders,
            next_id: AtomicU64::new(1),
            state_rx,
            _reconnect_task: reconnect_task,
        })
    }

    /// Current connection state.
    pub fn connection_state(&self) -> ConnectionState {
        *self.state_rx.borrow()
    }

    /// Watch connection state changes.
    pub fn watch_state(&self) -> watch::Receiver<ConnectionState> {
        self.state_rx.clone()
    }
}

/// Establish a single WebSocket connection, returning the live connection handles.
/// When the reader detects a disconnect, it clears `live` and signals via `state_tx`.
async fn establish_connection(
    addr: &str,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>>,
    session_senders: Arc<Mutex<HashMap<SessionId, broadcast::Sender<DaemonEvent>>>>,
    live: Arc<Mutex<Option<LiveConnection>>>,
    state_tx: watch::Sender<ConnectionState>,
) -> anyhow::Result<LiveConnection> {
    let url = format!("ws://{}", addr);
    let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await?;
    let (ws_sink, ws_source) = ws_stream.split();

    let (write_tx, mut write_rx) = mpsc::channel::<String>(256);

    let writer = tokio::spawn(async move {
        let mut sink = ws_sink;
        while let Some(text) = write_rx.recv().await {
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

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
                if let Some(tx) = pending.lock().await.remove(&id) {
                    let _ = tx.send(response);
                }
            } else if incoming.is_notification() {
                let method = incoming.method.as_deref().unwrap();
                if method == "session.event" {
                    if let Ok(params) = serde_json::from_value::<SessionEventParams>(incoming.params) {
                        let senders = session_senders.lock().await;
                        if let Some(tx) = senders.get(&params.session_id) {
                            let _ = tx.send(params.event);
                        }
                    }
                }
            }
        }

        // Connection lost — clear live connection, fail all pending, signal disconnect
        *live.lock().await = None;
        let mut pending = pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(WsResponse::err(0, "connection lost"));
        }
        let _ = state_tx.send(ConnectionState::Disconnected);
    });

    Ok(LiveConnection {
        write_tx,
        _reader: reader,
        _writer: writer,
    })
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

        let write_tx = {
            let live = self.live.lock().await;
            match live.as_ref() {
                Some(conn) => conn.write_tx.clone(),
                None => {
                    self.pending.lock().await.remove(&id);
                    return Err(anyhow::anyhow!("disconnected from daemon, reconnecting"));
                }
            }
        };

        write_tx
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

    async fn rest_call(
        &self,
        http_method: simply_rpc::HttpMethod,
        path: &str,
        body: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        // Multiplex REST calls over WebSocket as regular RPC calls.
        // Convention: method name = "REST.{METHOD} {path}"
        let method = format!("REST.{:?} {}", http_method, path);
        self.rpc_call(&method, body).await
    }
}
