//! Unified HTTP server for the daemon — REST + WebSocket on a single port.
//!
//! Uses axum for routing and WebSocket upgrades. REST routes are auto-registered
//! from `RestDispatcher`. The admin page and connection tracking are served alongside.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Method, StatusCode};
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use simply_rpc::{HttpMethod, RestDispatcher};

use crate::net::server::ConnectionTracker;

/// Server configuration.
pub struct RestConfig {
    pub rest_dispatcher: RestDispatcher,
    pub port: u16,
    pub tracker: ConnectionTracker,
    pub kill_tx: tokio::sync::mpsc::Sender<()>,
}

/// Shared state for axum handlers.
#[derive(Clone)]
struct AppState {
    rest_dispatcher: Arc<RestDispatcher>,
    tracker: ConnectionTracker,
    kill_tx: tokio::sync::mpsc::Sender<()>,
}

/// Starts the unified HTTP server (REST + admin).
pub async fn start(config: RestConfig) -> anyhow::Result<RestHandle> {
    let state = AppState {
        rest_dispatcher: Arc::new(config.rest_dispatcher),
        tracker: config.tracker,
        kill_tx: config.kill_tx,
    };

    let app = Router::new()
        // Admin routes
        .route("/", get(admin_page))
        .route("/admin", get(admin_page))
        .route("/admin/api/connections", get(admin_connections))
        // Catch-all: REST dispatch
        .fallback(rest_handler)
        .with_state(state);

    let addr = format!("127.0.0.1:{}", config.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(port = config.port, "HTTP server listening");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
            .await
            .ok();
    });

    Ok(RestHandle { _task: handle, _shutdown: shutdown_tx, port: config.port })
}

pub struct RestHandle {
    _task: JoinHandle<()>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
    port: u16,
}

impl RestHandle {
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
// REST catch-all handler
// ---------------------------------------------------------------------------

async fn rest_handler(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    tracing::debug!(method = %method, path = %path, "REST request");

    let http_method = match method {
        Method::GET => HttpMethod::Get,
        Method::POST => HttpMethod::Post,
        Method::PUT => HttpMethod::Put,
        Method::DELETE => HttpMethod::Delete,
        _ => return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response(),
    };

    // Read body for POST/PUT
    let body = match http_method {
        HttpMethod::Post | HttpMethod::Put => {
            let bytes = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
                Ok(b) => b,
                Err(_) => return (StatusCode::BAD_REQUEST, "failed to read body").into_response(),
            };
            if bytes.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
            }
        }
        _ => serde_json::Value::Null,
    };

    let rest_result = match state.rest_dispatcher.dispatch(http_method, &path, body).await {
        Some(r) => r,
        None => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };

    match rest_result.result {
        Ok(value) => {
            let mut response = if rest_result.meta.binary_response {
                // BinaryResponse: decode base64 data, serve raw bytes
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
