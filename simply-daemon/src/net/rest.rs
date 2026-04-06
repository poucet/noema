//! Unified HTTP server — REST + WebSocket on a single port.
//!
//! Uses axum for routing, REST dispatch, and WebSocket upgrades.
//! WebSocket connections upgrade at `/ws`. Everything else is REST dispatch.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{FromRequest, State};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::ConnectInfo;
use axum::http::{Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use simply_rpc::{HttpMethod, ServiceRouter};

use crate::net::server::ConnectionTracker;
use crate::net::protocol::*;

/// Server configuration.
pub struct ServerConfig {
    pub rest_dispatcher: Arc<ServiceRouter>,
    pub port: u16,
    pub tracker: ConnectionTracker,
    pub daemon_secret: String,
}

/// Shared state for axum handlers.
#[derive(Clone)]
struct AppState {
    rest_dispatcher: Arc<ServiceRouter>,
    tracker: ConnectionTracker,
    daemon_secret: Arc<str>,
}

/// Starts the unified server (REST + WS + admin).
pub async fn start(config: ServerConfig) -> anyhow::Result<ServerHandle> {
    let state = AppState {
        rest_dispatcher: config.rest_dispatcher,
        tracker: config.tracker,
        daemon_secret: Arc::from(config.daemon_secret.as_str()),
    };

    let app = Router::new()
        .route("/", get(admin_page))
        .route("/admin", get(admin_page))
        .route("/admin/api/connections", get(admin_connections))
        .route("/ws", get(ws_upgrade_handler))
        .fallback(rest_or_stream_handler)
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", config.port);
    let listener = TcpListener::bind(&addr).await?;
    let actual_port = listener.local_addr()?.port();
    tracing::info!(port = actual_port, "server listening");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
            .await
            .ok();
    });

    Ok(ServerHandle { _task: handle, _shutdown: shutdown_tx, port: actual_port })
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
// Auth middleware
// ---------------------------------------------------------------------------

async fn auth_middleware(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let path = req.uri().path();

    // Public routes: admin page, auth flows
    if path == "/" || path == "/admin" || path.starts_with("/auth/") {
        return next.run(req).await;
    }

    // All other routes require Bearer token
    let authorized = req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|token| token == state.daemon_secret.as_ref());

    if !authorized {
        return (StatusCode::UNAUTHORIZED, "invalid or missing daemon secret").into_response();
    }

    next.run(req).await
}

// ---------------------------------------------------------------------------
// Admin handlers
// ---------------------------------------------------------------------------

async fn admin_page() -> Html<&'static str> {
    Html(include_str!("../admin/admin.html"))
}

async fn admin_connections(State(state): State<AppState>) -> Json<Vec<crate::net::server::ConnectionInfo>> {
    Json(state.tracker.list().await)
}

// ---------------------------------------------------------------------------
// WebSocket upgrade handler
// ---------------------------------------------------------------------------

async fn ws_upgrade_handler(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    ws: axum::extract::WebSocketUpgrade,
) -> Response {
    let dispatcher = Arc::clone(&state.rest_dispatcher);
    let tracker = state.tracker.clone();
    ws.on_upgrade(move |socket| handle_ws_connection(dispatcher, socket, tracker, addr))
}

async fn handle_ws_connection(
    dispatcher: Arc<ServiceRouter>,
    socket: WebSocket,
    tracker: ConnectionTracker,
    addr: std::net::SocketAddr,
) {
    use futures_util::{SinkExt, StreamExt};

    tracing::info!(%addr, "WS client connected");
    let conn_id = tracker.add(addr).await;

    let (mut ws_sink, mut ws_source) = socket.split();
    let (write_tx, mut write_rx) = mpsc::channel::<String>(256);
    let mut input_sinks: std::collections::HashMap<String, mpsc::Sender<serde_json::Value>> = std::collections::HashMap::new();

    let writer_handle = tokio::spawn(async move {
        while let Some(text) = write_rx.recv().await {
            if ws_sink.send(Message::Text(text.into())).await.is_err() { break; }
        }
    });

    while let Some(msg) = ws_source.next().await {
        let text = match msg {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(e) => { tracing::error!(error = %e, "WS read error"); break; }
        };

        let incoming: WsIncoming = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => { tracing::warn!(error = %e, "invalid WS message"); continue; }
        };

        // Notifications (no id) with a method ending in ".input" are bidi stream input
        if incoming.is_notification() {
            if let Some(method) = &incoming.method {
                if let Some(base) = method.strip_suffix(".input") {
                    if let Some(sink) = input_sinks.get(base) {
                        let _ = sink.send(incoming.params).await;
                    }
                }
            }
            continue;
        }

        if !incoming.is_request() { continue; }

        let id = incoming.id.unwrap();
        let method = incoming.method.unwrap();
        let params = incoming.params;

        // Built-in: client identification
        if method == "client.identify" {
            let name = params.as_str()
                .or_else(|| params.get("name").and_then(|v| v.as_str()))
                .unwrap_or("unknown");
            tracker.set_name(conn_id, name.to_string()).await;
            tracing::info!(conn_id, name, "WS client identified");
            let response = WsResponse::ok(id, serde_json::json!({ "ok": true }));
            let text = serde_json::to_string(&response).unwrap_or_default();
            if write_tx.send(text).await.is_err() { break; }
            continue;
        }

        tracing::debug!(id, method = %method, "WS request");
        tracing::trace!(id, method = %method, params = %params, "WS request params");

        // Try stream dispatch first, then regular RPC
        let (mut response, sink) = if let Some(ws_result) = dispatcher.ws_dispatch_by_method(&method, params.clone(), write_tx.clone()).await {
            let r = match ws_result.result {
                Ok(v) => WsResponse::ok(0, v),
                Err(e) => WsResponse::err(0, e),
            };
            (r, Some(ws_result.input_sink))
        } else if let Some(rpc_result) = dispatcher.dispatch_by_method(&method, params).await {
            let r = match rpc_result {
                Ok(v) => WsResponse::ok(0, v),
                Err(e) => WsResponse::err(0, e),
            };
            (r, None)
        } else {
            (WsResponse::err(0, format!("unknown method: {method}")), None)
        };

        response.id = id;

        if let Some(sink) = sink {
            input_sinks.insert(method.clone(), sink);
        }

        let is_err = response.error.is_some();
        tracing::debug!(id, method = %method, error = is_err, "WS response");
        if is_err {
            tracing::debug!(id, error = ?response.error, "WS response error");
        }

        let text = serde_json::to_string(&response).unwrap_or_default();
        if write_tx.send(text).await.is_err() { break; }
    }

    writer_handle.abort();
    tracker.remove(conn_id).await;
    tracing::info!("WS client disconnected");
}

// ---------------------------------------------------------------------------
// Unified fallback — stream WS upgrade or REST dispatch
// ---------------------------------------------------------------------------

async fn rest_or_stream_handler(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Response {
    let path = req.uri().path().to_string();

    // Check if this is a WS upgrade for a stream route
    if req.headers().get("upgrade").and_then(|v| v.to_str().ok()) == Some("websocket")
        && state.rest_dispatcher.has_stream_route(&path)
    {
        // Re-extract the WebSocketUpgrade from the request
        let ws = match axum::extract::WebSocketUpgrade::from_request(req, &state).await {
            Ok(ws) => ws,
            Err(e) => return e.into_response(),
        };
        let dispatcher = Arc::clone(&state.rest_dispatcher);
        return ws.on_upgrade(move |socket| handle_stream_ws(dispatcher, path, socket));
    }

    rest_handler(state, req).await
}

/// Handle a WS connection for a stream route.
/// Uses `ServiceRouter::ws_dispatch` to set up the stream, then bridges
/// the WS connection: server events → text frames, client messages → input sink.
async fn handle_stream_ws(
    dispatcher: Arc<ServiceRouter>,
    path: String,
    socket: WebSocket,
) {
    use futures_util::{SinkExt, StreamExt};

    let (mut ws_sink, mut ws_source) = socket.split();
    let (write_tx, mut write_rx) = mpsc::channel::<String>(256);

    // Spawn writer: forwards queued messages to the WS sink
    let writer_handle = tokio::spawn(async move {
        while let Some(text) = write_rx.recv().await {
            if ws_sink.send(Message::Text(text.into())).await.is_err() { break; }
        }
        let _ = ws_sink.close().await;
    });

    // Dispatch the stream — this sets up the server pipeline and forwarders
    let result = dispatcher.ws_dispatch(&path, serde_json::Value::Null, write_tx.clone()).await;

    let input_sink = match result {
        Some(dr) => {
            // Send the initial response
            let response = match dr.result {
                Ok(v) => WsResponse::ok(0, v),
                Err(e) => WsResponse::err(0, e),
            };
            let text = serde_json::to_string(&response).unwrap_or_default();
            if write_tx.send(text).await.is_err() {
                writer_handle.abort();
                return;
            }
            dr.input_sink
        }
        None => {
            let err = WsResponse::err(0, "no stream handler for path");
            let text = serde_json::to_string(&err).unwrap_or_default();
            let _ = write_tx.send(text).await;
            writer_handle.abort();
            return;
        }
    };

    // Read client messages and forward to the input sink
    while let Some(msg) = ws_source.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    if input_sink.send(value).await.is_err() { break; }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                tracing::error!("stream WS read error: {e}");
                break;
            }
            _ => {}
        }
    }

    // Dropping input_sink closes the channel → pipeline stops
    drop(input_sink);
    writer_handle.abort();
    tracing::debug!(path, "stream WS disconnected");
}

async fn rest_handler(
    state: AppState,
    req: axum::extract::Request,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let content_type = req.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    tracing::info!(method = %method, path = %path, "REST request");

    let http_method = match method {
        Method::GET => HttpMethod::Get,
        Method::POST => HttpMethod::Post,
        Method::PUT => HttpMethod::Put,
        Method::DELETE => HttpMethod::Delete,
        _ => return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response(),
    };

    // Read raw body for POST/PUT
    let raw_bytes = match http_method {
        HttpMethod::Post | HttpMethod::Put => {
            match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
                Ok(b) => b,
                Err(_) => return (StatusCode::BAD_REQUEST, "failed to read body").into_response(),
            }
        }
        _ => axum::body::Bytes::new(),
    };

    // Check if this is a binary upload route
    let is_binary_upload = state.rest_dispatcher.is_binary_upload(http_method, &path);

    let body = if is_binary_upload {
        serde_json::to_value(simply_rpc::BinaryUpload::new(raw_bytes.to_vec(), content_type))
            .unwrap_or(serde_json::Value::Null)
    } else if raw_bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&raw_bytes).unwrap_or(serde_json::Value::Null)
    };

    let rest_result = match state.rest_dispatcher.dispatch(http_method, &path, body).await {
        Some(r) => r,
        None => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };

    match rest_result.result {
        Ok(value) => {
            let mut response = if rest_result.meta.binary_response {
                let br: simply_rpc::BinaryResponse = match serde_json::from_value(value) {
                    Ok(br) => br,
                    Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", &br.mime_type)
                    .header("Content-Length", br.data.len().to_string())
                    .body(Body::from(br.data))
                    .unwrap()
            } else {
                Json(value).into_response()
            };

            if rest_result.meta.immutable_cache {
                let etag = path.rsplit('/').next().unwrap_or("");
                let headers = response.headers_mut();
                headers.insert("Cache-Control", "public, max-age=31536000, immutable".parse().unwrap());
                headers.insert("ETag", format!("\"{etag}\"").parse().unwrap());
            }

            response
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("No such file") {
                (StatusCode::NOT_FOUND, msg).into_response()
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
            }
        }
    }
}
