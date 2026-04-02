//! WebSocket server — wraps a `DaemonApi` and serves it over WebSocket.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use simply_rpc::{Dispatcher, RpcService};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::api::*;
use super::protocol::*;

/// Starts a WebSocket server on the given port, serving the provided daemon.
pub async fn start(daemon: Arc<dyn DaemonApi>, port: u16) -> anyhow::Result<ServerHandle> {
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
                            let daemon = Arc::clone(&daemon);
                            tokio::spawn(handle_connection(daemon, stream));
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

async fn handle_connection(daemon: Arc<dyn DaemonApi>, stream: tokio::net::TcpStream) {
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

    // Build service dispatchers
    let session_svc = SessionApiService(daemon.clone());
    let asset_svc = AssetApiService(daemon.clone());

    let dispatcher = Dispatcher::new()
        .register(Arc::new(ConversationApiService(daemon.clone())))
        .register(Arc::new(McpApiService(daemon.clone())))
        .register(Arc::new(OAuthApiService(daemon.clone())))
        .register(Arc::new(ModelApiService(daemon.clone())))
        .register(Arc::new(VoiceApiService(daemon.clone())));

    let mut forwarders: HashMap<SessionId, JoinHandle<()>> = HashMap::new();

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
        let method = incoming.method.as_deref().unwrap();
        let params = incoming.params;

        let response = dispatch(
            id, method, params,
            &session_svc, &asset_svc, &dispatcher,
            &write_tx, &mut forwarders, &daemon,
        ).await;

        let text = serde_json::to_string(&response).unwrap_or_default();
        if write_tx.send(text).await.is_err() { break; }
    }

    for (_, h) in forwarders.drain() { h.abort(); }
    writer_handle.abort();
    tracing::info!("WS client disconnected");
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

async fn dispatch(
    id: u64,
    method: &str,
    params: serde_json::Value,
    session_svc: &SessionApiService<dyn DaemonApi>,
    asset_svc: &AssetApiService<dyn DaemonApi>,
    dispatcher: &Dispatcher,
    write_tx: &mpsc::Sender<String>,
    forwarders: &mut HashMap<SessionId, JoinHandle<()>>,
    daemon: &Arc<dyn DaemonApi>,
) -> WsResponse {
    // --- SessionApi (stream-producing) ---
    if let Some(dr) = session_svc.dispatch(method, params.clone()).await {
        for rx in dr.streams {
            let sid = extract_session_id(method, &dr.result, &params);
            spawn_event_forwarder(&sid, rx, write_tx.clone(), forwarders);
        }
        return to_ws_response(id, dr.result);
    }

    // --- AssetApi (base64 encoding, handled manually) ---
    if method.starts_with("asset.") {
        return dispatch_asset(id, method, params, daemon).await;
    }

    // --- Everything else via Dispatcher (McpApi, ConversationApi, OAuthApi, ModelApi, VoiceApi) ---
    let result = dispatcher.dispatch(method, params).await;
    to_ws_response(id, result)
}

/// Extract session ID from a stream method's result or params.
fn extract_session_id(
    method: &str,
    result: &simply_rpc::RpcResult,
    params: &serde_json::Value,
) -> SessionId {
    if method.ends_with("subscribe_session") {
        // subscribe_session takes session_id as the param
        serde_json::from_value(params.clone()).unwrap_or_else(|_| SessionId::new("unknown"))
    } else {
        // create_session and resume_session return SessionInfo
        result
            .as_ref()
            .ok()
            .and_then(|v| serde_json::from_value::<SessionInfo>(v.clone()).ok())
            .map(|info| info.id)
            .unwrap_or_else(|| SessionId::new("unknown"))
    }
}

/// Manual dispatch for AssetApi — needs base64 encode/decode.
async fn dispatch_asset(
    id: u64,
    method: &str,
    params: serde_json::Value,
    daemon: &Arc<dyn DaemonApi>,
) -> WsResponse {
    match method {
        "asset.store_asset" => {
            #[derive(serde::Deserialize)]
            struct P { data: String, media_type: String }
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e),
            };
            let bytes = match base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD, &p.data,
            ) {
                Ok(b) => b,
                Err(e) => return WsResponse::err(id, anyhow::anyhow!("invalid base64: {e}")),
            };
            match daemon.store_asset(bytes, &p.media_type).await {
                Ok(v) => WsResponse::ok(id, v),
                Err(e) => WsResponse::err(id, e),
            }
        }
        "asset.get_blob" => {
            let hash: simply_core::storage::types::BlobHash = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e),
            };
            match daemon.get_blob(&hash).await {
                Ok(data) => {
                    use base64::Engine;
                    WsResponse::ok(id, base64::engine::general_purpose::STANDARD.encode(&data))
                }
                Err(e) => WsResponse::err(id, e),
            }
        }
        _ => WsResponse::err(id, format!("unknown method: {method}")),
    }
}

fn to_ws_response(id: u64, result: simply_rpc::RpcResult) -> WsResponse {
    match result {
        Ok(v) => WsResponse { id, result: Some(v), error: None },
        Err(e) => WsResponse::err(id, e),
    }
}

// ---------------------------------------------------------------------------
// Event forwarder
// ---------------------------------------------------------------------------

pub fn spawn_event_forwarder(
    session_id: &SessionId,
    mut rx: tokio::sync::broadcast::Receiver<DaemonEvent>,
    write_tx: mpsc::Sender<String>,
    forwarders: &mut HashMap<SessionId, JoinHandle<()>>,
) {
    if let Some(old) = forwarders.remove(session_id) { old.abort(); }

    let sid = session_id.clone();
    let handle = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let notif = WsNotification {
                        method: "session.event".to_string(),
                        params: serde_json::to_value(SessionEventParams {
                            session_id: sid.clone(), event,
                        }).unwrap_or_default(),
                    };
                    if write_tx.send(serde_json::to_string(&notif).unwrap_or_default()).await.is_err() { break; }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(session_id = %sid, skipped = n, "event forwarder lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    forwarders.insert(session_id.clone(), handle);
}
