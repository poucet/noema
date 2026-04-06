//! WebSocket RPC client — generic transport with auto-reconnect.
//!
//! Provides `WsConnection<E>` which implements `RpcClient` for any event type `E`.
//! Automatically reconnects with exponential backoff when the connection drops.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc, oneshot, watch, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::protocol::*;

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

/// Callback that demuxes WS notifications into stream events.
/// Given (method, params), returns `Some((stream_id, event))` if this is a stream event.
pub type NotificationDemux<E> =
    Arc<dyn Fn(&str, serde_json::Value) -> Option<(String, E)> + Send + Sync>;

/// Active connection handles.
struct LiveConnection {
    write_tx: mpsc::Sender<String>,
    _reader: JoinHandle<()>,
    _writer: JoinHandle<()>,
}

/// Generic WebSocket RPC client. Parameterized over the stream event type.
pub struct WsConnection<E: Clone + Send + 'static> {
    live: Arc<Mutex<Option<LiveConnection>>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>>,
    stream_senders: Arc<Mutex<HashMap<String, broadcast::Sender<E>>>>,
    next_id: AtomicU64,
    state_rx: watch::Receiver<ConnectionState>,
    _reconnect_task: JoinHandle<()>,
}

impl<E: Clone + Send + 'static> WsConnection<E> {
    /// Connect to a server. Reconnects automatically on disconnect.
    ///
    /// `demux` converts WS notifications to `(stream_id, event)` for stream dispatch.
    pub async fn connect(addr: &str, demux: NotificationDemux<E>) -> anyhow::Result<Self> {
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let stream_senders: Arc<Mutex<HashMap<String, broadcast::Sender<E>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let live: Arc<Mutex<Option<LiveConnection>>> = Arc::new(Mutex::new(None));

        let (state_tx, state_rx) = watch::channel(ConnectionState::Connected);
        let initial = establish_connection(
            addr, Arc::clone(&pending), Arc::clone(&stream_senders),
            Arc::clone(&live), state_tx.clone(), Arc::clone(&demux),
        ).await?;
        *live.lock().await = Some(initial);

        let reconnect_task = {
            let addr = addr.to_string();
            let live = Arc::clone(&live);
            let pending = Arc::clone(&pending);
            let stream_senders = Arc::clone(&stream_senders);
            let state_tx = state_tx.clone();
            let mut state_rx = state_rx.clone();

            tokio::spawn(async move {
                loop {
                    loop {
                        if *state_rx.borrow_and_update() == ConnectionState::Disconnected {
                            break;
                        }
                        if state_rx.changed().await.is_err() {
                            return;
                        }
                    }

                    let _ = state_tx.send(ConnectionState::Reconnecting);
                    tracing::info!(addr = %addr, "Connection lost, starting reconnect");

                    let mut backoff = INITIAL_BACKOFF;
                    loop {
                        tokio::time::sleep(backoff).await;

                        match establish_connection(
                            &addr, Arc::clone(&pending), Arc::clone(&stream_senders),
                            Arc::clone(&live), state_tx.clone(), Arc::clone(&demux),
                        ).await {
                            Ok(conn) => {
                                *live.lock().await = Some(conn);
                                let _ = state_tx.send(ConnectionState::Connected);
                                tracing::info!(addr = %addr, "Reconnected");
                                break;
                            }
                            Err(e) => {
                                tracing::debug!(addr = %addr, backoff_ms = backoff.as_millis(), error = %e, "Reconnect failed");
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
            stream_senders,
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

async fn establish_connection<E: Clone + Send + 'static>(
    addr: &str,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>>,
    stream_senders: Arc<Mutex<HashMap<String, broadcast::Sender<E>>>>,
    live: Arc<Mutex<Option<LiveConnection>>>,
    state_tx: watch::Sender<ConnectionState>,
    demux: NotificationDemux<E>,
) -> anyhow::Result<LiveConnection> {
    let url = format!("ws://{}/ws", addr);
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
                if let Some((stream_id, event)) = demux(method, incoming.params) {
                    let senders = stream_senders.lock().await;
                    if let Some(tx) = senders.get(&stream_id) {
                        let _ = tx.send(event);
                    }
                }
            }
        }

        // Connection lost
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
impl<E: Clone + Send + Sync + 'static> crate::RpcClient for WsConnection<E> {
    type Stream = broadcast::Receiver<E>;

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
                    return Err(anyhow::anyhow!("disconnected, reconnecting"));
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
        let mut senders = self.stream_senders.lock().await;
        if let Some(tx) = senders.get(id) {
            return tx.subscribe();
        }
        let (tx, rx) = broadcast::channel(256);
        senders.insert(id.to_string(), tx);
        rx
    }

    async fn unregister_stream(&self, id: &str) {
        self.stream_senders.lock().await.remove(id);
    }

    async fn rest_call(
        &self,
        http_method: crate::HttpMethod,
        path: &str,
        body: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let method = format!("REST.{:?} {}", http_method, path);
        self.rpc_call(&method, body).await
    }
}
