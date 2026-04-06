//! Unified HTTP server — REST + WebSocket on a single port.
//!
//! Uses axum for routing, REST dispatch, and WebSocket upgrades.
//! WebSocket connections upgrade at `/ws`. Everything else is REST dispatch.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::ConnectInfo;
use axum::http::{Method, StatusCode};
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use simply_rpc::{HttpMethod, RestDispatcher};

use crate::net::server::{ConnectionTracker, DispatchFn};
use crate::net::protocol::*;

/// Server configuration.
pub struct ServerConfig {
    pub rest_dispatcher: RestDispatcher,
    pub ws_dispatch: Option<DispatchFn>,
    pub port: u16,
    pub tracker: ConnectionTracker,
}

/// Shared state for axum handlers.
#[derive(Clone)]
struct AppState {
    rest_dispatcher: Arc<RestDispatcher>,
    ws_dispatch: Option<DispatchFn>,
    tracker: ConnectionTracker,
}

/// Starts the unified server (REST + WS + admin).
pub async fn start(config: ServerConfig) -> anyhow::Result<ServerHandle> {
    let state = AppState {
        rest_dispatcher: Arc::new(config.rest_dispatcher),
        ws_dispatch: config.ws_dispatch,
        tracker: config.tracker,
    };

    let app = Router::new()
        .route("/", get(admin_page))
        .route("/admin", get(admin_page))
        .route("/admin/api/connections", get(admin_connections))
        .route("/ws", get(ws_upgrade_handler))
        .fallback(rest_handler)
        .with_state(state);

    let addr = format!("127.0.0.1:{}", config.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(port = config.port, "server listening");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
            .await
            .ok();
    });

    Ok(ServerHandle { _task: handle, _shutdown: shutdown_tx, port: config.port })
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
    let dispatch = match state.ws_dispatch {
        Some(ref d) => Arc::clone(d),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "WebSocket not available").into_response(),
    };
    let tracker = state.tracker.clone();
    ws.on_upgrade(move |socket| handle_ws_connection(dispatch, socket, tracker, addr))
}

async fn handle_ws_connection(
    dispatch: DispatchFn,
    socket: WebSocket,
    tracker: ConnectionTracker,
    addr: std::net::SocketAddr,
) {
    use futures_util::{SinkExt, StreamExt};

    tracing::info!(%addr, "WS client connected");
    let conn_id = tracker.add(addr).await;

    let (mut ws_sink, mut ws_source) = socket.split();
    let (write_tx, mut write_rx) = mpsc::channel::<String>(256);

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

        let mut response = dispatch(method.clone(), params, write_tx.clone()).await;
        response.id = id;

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
// Unified handler — REST fallback
// ---------------------------------------------------------------------------

async fn rest_handler(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let content_type = req.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    tracing::debug!(method = %method, path = %path, "request");

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
