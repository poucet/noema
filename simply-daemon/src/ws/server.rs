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

/// Starts a WebSocket server on the given port, serving the provided daemon.
/// Returns a handle that can be used to shut down the server.
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
                        Err(e) => {
                            tracing::error!(error = %e, "WS accept error");
                        }
                    }
                }
            }
        }
    });

    Ok(ServerHandle {
        _task: handle,
        _shutdown: shutdown_tx,
        port,
    })
}

pub struct ServerHandle {
    _task: JoinHandle<()>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
    port: u16,
}

impl ServerHandle {
    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Handle a single WebSocket connection.
async fn handle_connection(
    daemon: Arc<dyn DaemonApi>,
    stream: tokio::net::TcpStream,
) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::error!(error = %e, "WS handshake failed");
            return;
        }
    };

    let (ws_sink, mut ws_source) = ws_stream.split();

    // Writer task: receives serialized messages via channel and writes to WS
    let (write_tx, mut write_rx) = mpsc::channel::<String>(256);
    let writer_handle = tokio::spawn(async move {
        let mut sink = ws_sink;
        while let Some(text) = write_rx.recv().await {
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Track event forwarder tasks so we can clean up on disconnect
    let mut forwarders: HashMap<SessionId, JoinHandle<()>> = HashMap::new();

    // Read loop: process requests
    while let Some(msg) = ws_source.next().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Close(_)) => break,
            Ok(_) => continue, // ignore binary/ping/pong
            Err(e) => {
                tracing::error!(error = %e, "WS read error");
                break;
            }
        };

        let incoming: WsIncoming = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "invalid WS message");
                continue;
            }
        };

        if !incoming.is_request() {
            continue; // server only handles requests from clients
        }

        let id = incoming.id.unwrap();
        let method = incoming.method.as_deref().unwrap();

        let response = dispatch(&daemon, id, method, incoming.params, &write_tx, &mut forwarders).await;

        let text = serde_json::to_string(&response).unwrap_or_default();
        if write_tx.send(text).await.is_err() {
            break;
        }
    }

    // Connection closed — clean up forwarders
    for (_, handle) in forwarders.drain() {
        handle.abort();
    }
    writer_handle.abort();
    tracing::info!("WS client disconnected");
}

/// Dispatch a request to the appropriate DaemonApi method.
async fn dispatch(
    daemon: &Arc<dyn DaemonApi>,
    id: u64,
    method: &str,
    params: serde_json::Value,
    write_tx: &mpsc::Sender<String>,
    forwarders: &mut HashMap<SessionId, JoinHandle<()>>,
) -> WsResponse {
    match method {
        // --- SessionApi ---
        "session.create" => {
            let opts: CreateSessionOptions = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.create_session(opts).await {
                Ok((info, rx)) => {
                    spawn_event_forwarder(&info.id, rx, write_tx.clone(), forwarders);
                    WsResponse::ok(id, &info)
                }
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "session.resume" => {
            let sid: SessionId = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.resume_session(&sid).await {
                Ok((info, rx)) => {
                    spawn_event_forwarder(&info.id, rx, write_tx.clone(), forwarders);
                    WsResponse::ok(id, &info)
                }
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "session.subscribe" => {
            let sid: SessionId = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.subscribe_session(&sid).await {
                Ok(rx) => {
                    spawn_event_forwarder(&sid, rx, write_tx.clone(), forwarders);
                    WsResponse::ok(id, true)
                }
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "session.close" => rpc_call!(id, params, |sid: SessionId| daemon.close_session(&sid)),
        "session.close_all" => match daemon.close_all_sessions().await {
            Ok(()) => WsResponse::ok(id, true),
            Err(e) => WsResponse::err(id, e.to_string()),
        },
        "session.send_message" => {
            #[derive(Deserialize)]
            struct P { session_id: SessionId, message: UserMessage }
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.send_message(&p.session_id, p.message).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "session.get_messages" => rpc_call!(id, params, |sid: SessionId| daemon.get_messages(&sid)),
        "session.set_model" => {
            #[derive(Deserialize)]
            struct P { session_id: SessionId, model_id: String }
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.set_model(&p.session_id, &p.model_id).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "session.set_persistence" => {
            #[derive(Deserialize)]
            struct P { session_id: SessionId, persistence: Persistence }
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.set_persistence(&p.session_id, p.persistence).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "session.seed_context" => {
            #[derive(Deserialize)]
            struct P { session_id: SessionId, messages: Vec<SeedMessage> }
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.seed_context(&p.session_id, p.messages).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "session.list" => match daemon.list_sessions().await {
            Ok(v) => WsResponse::ok(id, v),
            Err(e) => WsResponse::err(id, e.to_string()),
        },
        "session.reload" => rpc_call!(id, params, |sid: SessionId| daemon.reload(&sid)),
        "session.push_event" => {
            let event: InboundEvent = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.push_event(event).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }

        // --- ConversationApi ---
        "conversation.create" => {
            #[derive(Deserialize)]
            struct P { name: Option<String> }
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.create_conversation(p.name.as_deref()).await {
                Ok(cid) => WsResponse::ok(id, cid),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "conversation.list" => match daemon.list_conversations().await {
            Ok(v) => WsResponse::ok(id, v),
            Err(e) => WsResponse::err(id, e.to_string()),
        },
        "conversation.delete" => {
            let cid: ConversationId = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.delete_conversation(&cid).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "conversation.rename" => {
            #[derive(Deserialize)]
            struct P { id: ConversationId, name: String }
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.rename_conversation(&p.id, &p.name).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }

        // --- AssetApi ---
        "asset.store" => {
            #[derive(Deserialize)]
            struct P { data: String, media_type: String } // data is base64
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            let bytes = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &p.data) {
                Ok(b) => b,
                Err(e) => return WsResponse::err(id, format!("invalid base64: {e}")),
            };
            match daemon.store_asset(bytes, &p.media_type).await {
                Ok(asset_id) => WsResponse::ok(id, asset_id),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "asset.get_blob" => {
            let hash: simply_core::storage::types::BlobHash = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.get_blob(&hash).await {
                Ok(data) => {
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                    WsResponse::ok(id, b64)
                }
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }

        // --- McpApi ---
        "mcp.list_servers" => match daemon.list_mcp_servers().await {
            Ok(v) => WsResponse::ok(id, v),
            Err(e) => WsResponse::err(id, e.to_string()),
        },
        "mcp.add_server" => {
            let req: AddMcpServerRequest = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.add_mcp_server(req).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "mcp.remove_server" => rpc_call!(id, params, |sid: String| daemon.remove_mcp_server(&sid)),
        "mcp.connect" => rpc_call!(id, params, |sid: String| daemon.connect_mcp_server(&sid)),
        "mcp.disconnect" => rpc_call!(id, params, |sid: String| daemon.disconnect_mcp_server(&sid)),
        "mcp.get_tools" => rpc_call!(id, params, |sid: String| daemon.get_mcp_server_tools(&sid)),
        "mcp.test" => rpc_call!(id, params, |sid: String| daemon.test_mcp_server(&sid)),
        "mcp.update_settings" => {
            #[derive(Deserialize)]
            struct P { server_id: String, request: UpdateMcpServerRequest }
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.update_mcp_server_settings(&p.server_id, p.request).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "mcp.stop_retry" => rpc_call!(id, params, |sid: String| daemon.stop_mcp_retry(&sid)),
        "mcp.start_retry" => rpc_call!(id, params, |sid: String| daemon.start_mcp_retry(&sid)),

        // --- OAuthApi ---
        "oauth.start" => rpc_call!(id, params, |sid: String| daemon.start_oauth(&sid)),
        "oauth.complete" => {
            #[derive(Deserialize)]
            struct P { server_id: String, code: String, state: String }
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.complete_oauth(&p.server_id, &p.code, &p.state).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }
        "oauth.complete_with_code" => {
            #[derive(Deserialize)]
            struct P { server_id: String, code: String }
            let p: P = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return WsResponse::err(id, e.to_string()),
            };
            match daemon.complete_oauth_with_code(&p.server_id, &p.code).await {
                Ok(()) => WsResponse::ok(id, true),
                Err(e) => WsResponse::err(id, e.to_string()),
            }
        }

        // --- ModelApi ---
        "model.list" => match daemon.list_models().await {
            Ok(v) => WsResponse::ok(id, v),
            Err(e) => WsResponse::err(id, e.to_string()),
        },
        "model.list_providers" => {
            let providers = daemon.list_providers().await;
            WsResponse::ok(id, providers)
        }
        "model.default_id" => {
            let model_id = daemon.default_model_id().await;
            WsResponse::ok(id, model_id)
        }
        "model.set_default" => rpc_call!(id, params, |mid: String| daemon.set_default_model(&mid)),

        _ => WsResponse::err(id, format!("unknown method: {method}")),
    }
}

/// Spawn a task that forwards DaemonEvents from a broadcast receiver
/// into WS notifications. Replaces any existing forwarder for the same session.
fn spawn_event_forwarder(
    session_id: &SessionId,
    mut rx: tokio::sync::broadcast::Receiver<DaemonEvent>,
    write_tx: mpsc::Sender<String>,
    forwarders: &mut HashMap<SessionId, JoinHandle<()>>,
) {
    // Abort previous forwarder for this session if any
    if let Some(old) = forwarders.remove(session_id) {
        old.abort();
    }

    let sid = session_id.clone();
    let handle = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let notification = WsNotification {
                        method: "session.event".to_string(),
                        params: serde_json::to_value(SessionEventParams {
                            session_id: sid.clone(),
                            event,
                        })
                        .unwrap_or_default(),
                    };
                    let text = serde_json::to_string(&notification).unwrap_or_default();
                    if write_tx.send(text).await.is_err() {
                        break; // connection closed
                    }
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

/// Helper macro for simple single-param RPC calls.
macro_rules! rpc_call {
    ($id:expr, $params:expr, |$p:ident : $T:ty| $call:expr) => {{
        let $p: $T = match serde_json::from_value($params) {
            Ok(v) => v,
            Err(e) => return WsResponse::err($id, e.to_string()),
        };
        match $call.await {
            Ok(v) => WsResponse::ok($id, v),
            Err(e) => WsResponse::err($id, e.to_string()),
        }
    }};
}
use rpc_call;
