//! WebSocket server — wraps a `DaemonApi` and serves it over WebSocket.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::api::*;
use super::protocol::*;

// ---------------------------------------------------------------------------
// Helper macros (must be defined before use)
// ---------------------------------------------------------------------------

/// Deserialize params or return error response.
macro_rules! de {
    ($id:expr, $params:expr) => {
        match serde_json::from_value($params) {
            Ok(v) => v,
            Err(e) => return WsResponse::err($id, e),
        }
    };
}

/// Single-param RPC: deserialize one value, call with &ref, return serialized result.
macro_rules! rpc {
    ($id:expr, $params:expr, |$p:ident : $T:ty| $call:expr) => {{
        let $p: $T = de!($id, $params);
        match $call.await {
            Ok(v) => WsResponse::ok($id, v),
            Err(e) => WsResponse::err($id, e),
        }
    }};
}

/// RPC returning Result<()> — returns true on success.
macro_rules! rpc_unit {
    ($id:expr, $call:expr) => {
        match $call.await {
            Ok(()) => WsResponse::ok($id, true),
            Err(e) => WsResponse::err($id, e),
        }
    };
}

/// RPC returning Result<T> — serialize the value.
macro_rules! rpc_val {
    ($id:expr, $call:expr) => {
        match $call.await {
            Ok(v) => WsResponse::ok($id, v),
            Err(e) => WsResponse::err($id, e),
        }
    };
}

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

        let response = dispatch(
            &daemon, id, method, incoming.params, &write_tx, &mut forwarders,
        ).await;

        let text = serde_json::to_string(&response).unwrap_or_default();
        if write_tx.send(text).await.is_err() { break; }
    }

    for (_, h) in forwarders.drain() { h.abort(); }
    writer_handle.abort();
    tracing::info!("WS client disconnected");
}

// ---------------------------------------------------------------------------
// Dispatch — maps method strings to DaemonApi trait calls
// ---------------------------------------------------------------------------

async fn dispatch(
    daemon: &Arc<dyn DaemonApi>,
    id: u64,
    method: &str,
    params: serde_json::Value,
    write_tx: &mpsc::Sender<String>,
    forwarders: &mut HashMap<SessionId, JoinHandle<()>>,
) -> WsResponse {
    match method {
        // --- SessionApi (stream methods) ---
        "session.create_session" => {
            let opts: CreateSessionOptions = de!(id, params);
            match daemon.create_session(opts).await {
                Ok((info, rx)) => {
                    spawn_event_forwarder(&info.id, rx, write_tx.clone(), forwarders);
                    WsResponse::ok(id, &info)
                }
                Err(e) => WsResponse::err(id, e),
            }
        }
        "session.resume_session" => {
            let sid: SessionId = de!(id, params);
            match daemon.resume_session(&sid).await {
                Ok((info, rx)) => {
                    spawn_event_forwarder(&info.id, rx, write_tx.clone(), forwarders);
                    WsResponse::ok(id, &info)
                }
                Err(e) => WsResponse::err(id, e),
            }
        }
        "session.subscribe_session" => {
            let sid: SessionId = de!(id, params);
            match daemon.subscribe_session(&sid).await {
                Ok(rx) => {
                    spawn_event_forwarder(&sid, rx, write_tx.clone(), forwarders);
                    WsResponse::ok(id, true)
                }
                Err(e) => WsResponse::err(id, e),
            }
        }

        // --- SessionApi (simple) ---
        "session.close_session" => rpc!(id, params, |sid: SessionId| daemon.close_session(&sid)),
        "session.close_all_sessions" => rpc_unit!(id, daemon.close_all_sessions()),
        "session.send_message" => {
            #[derive(serde::Deserialize)] struct P { session_id: SessionId, message: UserMessage }
            let p: P = de!(id, params);
            rpc_unit!(id, daemon.send_message(&p.session_id, p.message))
        }
        "session.get_messages" => rpc!(id, params, |sid: SessionId| daemon.get_messages(&sid)),
        "session.set_model" => {
            #[derive(serde::Deserialize)] struct P { session_id: SessionId, model_id: String }
            let p: P = de!(id, params);
            rpc_unit!(id, daemon.set_model(&p.session_id, &p.model_id))
        }
        "session.set_persistence" => {
            #[derive(serde::Deserialize)] struct P { session_id: SessionId, persistence: Persistence }
            let p: P = de!(id, params);
            rpc_unit!(id, daemon.set_persistence(&p.session_id, p.persistence))
        }
        "session.seed_context" => {
            #[derive(serde::Deserialize)] struct P { session_id: SessionId, messages: Vec<SeedMessage> }
            let p: P = de!(id, params);
            rpc_unit!(id, daemon.seed_context(&p.session_id, p.messages))
        }
        "session.list_sessions" => rpc_val!(id, daemon.list_sessions()),
        "session.reload" => rpc!(id, params, |sid: SessionId| daemon.reload(&sid)),
        "session.push_event" => {
            let event: InboundEvent = de!(id, params);
            rpc_unit!(id, daemon.push_event(event))
        }

        // --- ConversationApi ---
        "conversation.create_conversation" => {
            #[derive(serde::Deserialize)] struct P { name: Option<String> }
            let p: P = de!(id, params);
            rpc_val!(id, daemon.create_conversation(p.name.as_deref()))
        }
        "conversation.list_conversations" => rpc_val!(id, daemon.list_conversations()),
        "conversation.delete_conversation" => rpc!(id, params, |cid: ConversationId| daemon.delete_conversation(&cid)),
        "conversation.rename_conversation" => {
            #[derive(serde::Deserialize)] struct P { conversation_id: ConversationId, name: String }
            let p: P = de!(id, params);
            rpc_unit!(id, daemon.rename_conversation(&p.conversation_id, &p.name))
        }

        // --- AssetApi ---
        "asset.store_asset" => {
            #[derive(serde::Deserialize)] struct P { data: String, media_type: String }
            let p: P = de!(id, params);
            let bytes = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &p.data) {
                Ok(b) => b,
                Err(e) => return WsResponse::err(id, anyhow::anyhow!("invalid base64: {e}")),
            };
            rpc_val!(id, daemon.store_asset(bytes, &p.media_type))
        }
        "asset.get_blob" => {
            let hash: simply_core::storage::types::BlobHash = de!(id, params);
            match daemon.get_blob(&hash).await {
                Ok(data) => {
                    use base64::Engine;
                    WsResponse::ok(id, base64::engine::general_purpose::STANDARD.encode(&data))
                }
                Err(e) => WsResponse::err(id, e),
            }
        }

        // --- McpApi ---
        "mcp.list_mcp_servers" => rpc_val!(id, daemon.list_mcp_servers()),
        "mcp.add_mcp_server" => {
            let req: AddMcpServerRequest = de!(id, params);
            rpc_unit!(id, daemon.add_mcp_server(req))
        }
        "mcp.remove_mcp_server" => rpc!(id, params, |sid: String| daemon.remove_mcp_server(&sid)),
        "mcp.connect_mcp_server" => rpc!(id, params, |sid: String| daemon.connect_mcp_server(&sid)),
        "mcp.disconnect_mcp_server" => rpc!(id, params, |sid: String| daemon.disconnect_mcp_server(&sid)),
        "mcp.get_mcp_server_tools" => rpc!(id, params, |sid: String| daemon.get_mcp_server_tools(&sid)),
        "mcp.test_mcp_server" => rpc!(id, params, |sid: String| daemon.test_mcp_server(&sid)),
        "mcp.update_mcp_server_settings" => {
            #[derive(serde::Deserialize)] struct P { server_id: String, request: UpdateMcpServerRequest }
            let p: P = de!(id, params);
            rpc_unit!(id, daemon.update_mcp_server_settings(&p.server_id, p.request))
        }
        "mcp.stop_mcp_retry" => rpc!(id, params, |sid: String| daemon.stop_mcp_retry(&sid)),
        "mcp.start_mcp_retry" => rpc!(id, params, |sid: String| daemon.start_mcp_retry(&sid)),

        // --- OAuthApi ---
        "oauth.start_oauth" => rpc!(id, params, |sid: String| daemon.start_oauth(&sid)),
        "oauth.complete_oauth" => {
            #[derive(serde::Deserialize)] struct P { server_id: String, code: String, state: String }
            let p: P = de!(id, params);
            rpc_unit!(id, daemon.complete_oauth(&p.server_id, &p.code, &p.state))
        }
        "oauth.complete_oauth_with_code" => {
            #[derive(serde::Deserialize)] struct P { server_id: String, code: String }
            let p: P = de!(id, params);
            rpc_unit!(id, daemon.complete_oauth_with_code(&p.server_id, &p.code))
        }

        // --- ModelApi ---
        "model.list_models" => rpc_val!(id, daemon.list_models()),
        "model.list_providers" => WsResponse::ok(id, daemon.list_providers().await),
        "model.default_model_id" => WsResponse::ok(id, daemon.default_model_id().await),
        "model.set_default_model" => rpc!(id, params, |mid: String| daemon.set_default_model(&mid)),

        _ => WsResponse::err(id, format!("unknown method: {method}")),
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

