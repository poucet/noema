//! Admin dashboard — serves a browser-based UI for daemon management.
//!
//! Provides `/health`, `/kill`, admin page at `/`, and connection API.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use hyper::{Response, StatusCode};
use http_body_util::Full;
use tokio::sync::mpsc;

use crate::ws::server::ConnectionTracker;

type HyperResponse = Response<Full<hyper::body::Bytes>>;

/// Route handler type used by the REST server for extensible routes.
pub type RouteHandler = Arc<
    dyn Fn(
            &str, // path
            &str, // HTTP method
        ) -> Option<Pin<Box<dyn Future<Output = HyperResponse> + Send>>>
        + Send
        + Sync,
>;

/// Build the admin route handler.
///
/// Returns the handler and a receiver that fires when `/kill` is called.
pub fn routes(tracker: ConnectionTracker) -> (RouteHandler, mpsc::Receiver<()>) {
    let (kill_tx, kill_rx) = mpsc::channel(1);
    let kill_tx = Arc::new(kill_tx);

    let handler: RouteHandler = Arc::new(move |path, method| {
        match (path, method) {
            ("/health", _) => {
                Some(Box::pin(std::future::ready(json_response(
                    StatusCode::OK,
                    &serde_json::json!({ "status": "ok" }),
                ))))
            }
            ("/kill", "POST") => {
                let kill_tx = Arc::clone(&kill_tx);
                Some(Box::pin(async move {
                    tracing::info!("Kill requested via REST");
                    let _ = kill_tx.send(()).await;
                    json_response(StatusCode::OK, &serde_json::json!({ "status": "shutting_down" }))
                }))
            }
            ("/" | "/admin" | "/admin/", _) => {
                Some(Box::pin(std::future::ready(index_page())))
            }
            ("/admin/api/connections", _) => {
                let tracker = tracker.clone();
                Some(Box::pin(async move { connections_json(&tracker).await }))
            }
            _ => None,
        }
    });

    (handler, kill_rx)
}

fn index_page() -> HyperResponse {
    const HTML: &str = include_str!("admin.html");
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Full::new(hyper::body::Bytes::from(HTML)))
        .unwrap()
}

async fn connections_json(tracker: &ConnectionTracker) -> HyperResponse {
    let connections = tracker.list().await;
    let json = serde_json::to_vec(&connections).unwrap_or_default();
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Full::new(hyper::body::Bytes::from(json)))
        .unwrap()
}

fn json_response(status: StatusCode, value: &serde_json::Value) -> HyperResponse {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::new(hyper::body::Bytes::from(
            serde_json::to_vec(value).unwrap_or_default(),
        )))
        .unwrap()
}
