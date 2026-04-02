//! WebSocket server — generic RPC server over WebSocket.
//!
//! Knows nothing about specific API traits. Takes a dispatch function
//! that the caller wires up with the appropriate services.

use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use super::protocol::*;

/// Dispatch function signature.
///
/// Called for each incoming RPC request. The `write_tx` sender allows the
/// dispatch logic to send async notifications (e.g. session event streams).
///
/// Built by the caller (e.g. `discovery.rs`) from specific services.
pub type DispatchFn = Arc<
    dyn Fn(
            String,                    // method
            serde_json::Value,         // params
            mpsc::Sender<String>,      // write channel for notifications
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = WsResponse> + Send>>
        + Send
        + Sync,
>;

/// Starts a WebSocket server on the given port using the provided dispatch function.
pub async fn start(dispatch: DispatchFn, port: u16) -> anyhow::Result<ServerHandle> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(port, "WebSocket server listening");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            tracing::info!(%addr, "WS client connected");
                            let dispatch = Arc::clone(&dispatch);
                            tokio::spawn(handle_connection(dispatch, stream));
                        }
                        Err(e) => tracing::error!(error = %e, "WS accept error"),
                    }
                }
            }
        }
    });

    Ok(ServerHandle { _task: handle, _shutdown: shutdown_tx, port })
}

pub struct ServerHandle {
    _task: JoinHandle<()>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
    port: u16,
}

impl ServerHandle {
    pub fn port(&self) -> u16 { self.port }
}

// ---------------------------------------------------------------------------
// Connection handler
// ---------------------------------------------------------------------------

async fn handle_connection(dispatch: DispatchFn, stream: tokio::net::TcpStream) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => { tracing::error!(error = %e, "WS handshake failed"); return; }
    };

    let (ws_sink, mut ws_source) = ws_stream.split();

    let (write_tx, mut write_rx) = mpsc::channel::<String>(256);
    let writer_handle = tokio::spawn(async move {
        let mut sink = ws_sink;
        while let Some(text) = write_rx.recv().await {
            if sink.send(Message::Text(text.into())).await.is_err() { break; }
        }
    });

    while let Some(msg) = ws_source.next().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(e) => { tracing::error!(error = %e, "WS read error"); break; }
        };

        let incoming: WsIncoming = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(e) => { tracing::warn!(error = %e, "invalid WS message"); continue; }
        };

        if !incoming.is_request() { continue; }

        let id = incoming.id.unwrap();
        let method = incoming.method.unwrap();
        let params = incoming.params;

        let response = dispatch(method, params, write_tx.clone()).await;

        let text = serde_json::to_string(&response).unwrap_or_default();
        if write_tx.send(text).await.is_err() { break; }
    }

    writer_handle.abort();
    tracing::info!("WS client disconnected");
}
