//! WebSocket RPC client with auto-reconnect.
//!
//! `WsConnection` handles the WS transport: request/response, notifications,
//! and bidirectional streams. All notifications are routed by method name
//! to registered sinks.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, watch, Mutex};
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

const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const BACKOFF_MULTIPLIER: u32 = 2;

struct LiveConnection {
    write_tx: mpsc::Sender<String>,
    _reader: JoinHandle<()>,
    _writer: JoinHandle<()>,
}

/// WebSocket RPC client. Handles request/response and notification routing.
pub struct WsConnection {
    live: Arc<Mutex<Option<LiveConnection>>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>>,
    /// Notification sinks keyed by method name.
    sinks: Arc<Mutex<HashMap<String, mpsc::Sender<serde_json::Value>>>>,
    next_id: AtomicU64,
    state_rx: watch::Receiver<ConnectionState>,
    _reconnect_task: JoinHandle<()>,
}

impl WsConnection {
    /// Connect to a server. Reconnects automatically on disconnect.
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let sinks: Arc<Mutex<HashMap<String, mpsc::Sender<serde_json::Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let live: Arc<Mutex<Option<LiveConnection>>> = Arc::new(Mutex::new(None));

        let (state_tx, state_rx) = watch::channel(ConnectionState::Connected);
        let initial = establish_connection(
            addr, Arc::clone(&pending), Arc::clone(&sinks),
            Arc::clone(&live), state_tx.clone(),
        ).await?;
        *live.lock().await = Some(initial);

        let reconnect_task = {
            let addr = addr.to_string();
            let live = Arc::clone(&live);
            let pending = Arc::clone(&pending);
            let sinks = Arc::clone(&sinks);
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
                            &addr, Arc::clone(&pending), Arc::clone(&sinks),
                            Arc::clone(&live), state_tx.clone(),
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
            sinks,
            next_id: AtomicU64::new(1),
            state_rx,
            _reconnect_task: reconnect_task,
        })
    }

    pub fn connection_state(&self) -> ConnectionState {
        *self.state_rx.borrow()
    }

    pub fn watch_state(&self) -> watch::Receiver<ConnectionState> {
        self.state_rx.clone()
    }

    /// Get a clone of the write channel for sending raw WS messages.
    pub async fn write_tx(&self) -> Option<mpsc::Sender<String>> {
        self.live.lock().await.as_ref().map(|c| c.write_tx.clone())
    }

    /// Send an RPC request and wait for the response.
    pub async fn rpc_call(&self, method: &str, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
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

    /// Register a notification sink for a specific method name.
    /// Returns a receiver for incoming notifications matching that method.
    pub async fn register_sink(&self, method: &str) -> mpsc::Receiver<serde_json::Value> {
        let (tx, rx) = mpsc::channel(64);
        self.sinks.lock().await.insert(method.to_string(), tx);
        rx
    }

    /// Unregister a notification sink.
    pub async fn unregister_sink(&self, method: &str) {
        self.sinks.lock().await.remove(method);
    }
}

async fn establish_connection(
    addr: &str,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>>,
    sinks: Arc<Mutex<HashMap<String, mpsc::Sender<serde_json::Value>>>>,
    live: Arc<Mutex<Option<LiveConnection>>>,
    state_tx: watch::Sender<ConnectionState>,
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
                let sinks = sinks.lock().await;
                if let Some(tx) = sinks.get(method) {
                    let _ = tx.send(incoming.params).await;
                }
            }
        }

        // Connection lost — clean up
        *live.lock().await = None;
        let mut pending = pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(WsResponse::err(0, "connection lost"));
        }
        // Close all sinks so spawned tasks exit
        sinks.lock().await.clear();
        let _ = state_tx.send(ConnectionState::Disconnected);
    });

    Ok(LiveConnection {
        write_tx,
        _reader: reader,
        _writer: writer,
    })
}
