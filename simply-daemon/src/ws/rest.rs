//! Lightweight HTTP REST server for the daemon.
//!
//! Auto-routes `#[rpc(rest_get)]` methods from service metadata.
//! Extensible via optional route handler (used by admin module).

use std::sync::Arc;

use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use simply_rpc::Dispatcher;

use crate::admin::RouteHandler;

/// REST server configuration.
pub struct RestConfig {
    pub dispatcher: Dispatcher,
    pub port: u16,
    /// Optional extra route handler (e.g. admin dashboard).
    pub extra_routes: Option<RouteHandler>,
}

/// Starts a REST HTTP server.
pub async fn start(config: RestConfig) -> anyhow::Result<RestHandle> {
    let addr = format!("127.0.0.1:{}", config.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(port = config.port, "REST server listening");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
    let dispatcher = Arc::new(config.dispatcher);
    let extra_routes: Arc<Option<RouteHandler>> = Arc::new(config.extra_routes);

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let dispatcher = Arc::clone(&dispatcher);
                            let extra_routes = Arc::clone(&extra_routes);
                            tokio::spawn(async move {
                                let service = service_fn(move |req| {
                                    let dispatcher = Arc::clone(&dispatcher);
                                    let extra_routes = Arc::clone(&extra_routes);
                                    async move { handle_request(req, &dispatcher, &extra_routes).await }
                                });
                                if let Err(e) = http1::Builder::new()
                                    .serve_connection(TokioIo::new(stream), service)
                                    .await
                                {
                                    tracing::debug!(error = %e, "HTTP connection error");
                                }
                            });
                        }
                        Err(e) => tracing::error!(error = %e, "REST accept error"),
                    }
                }
            }
        }
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

type HyperResponse = Response<Full<hyper::body::Bytes>>;

async fn handle_request(
    req: Request<Incoming>,
    dispatcher: &Dispatcher,
    extra_routes: &Option<RouteHandler>,
) -> Result<HyperResponse, std::convert::Infallible> {
    let path = req.uri().path().to_string();
    let method = req.method().as_str();

    tracing::debug!(method, path = %path, "REST request");

    // --- Extra routes (admin, health, kill, etc.) ---
    if let Some(handler) = extra_routes {
        if let Some(fut) = handler(&path, method) {
            return Ok(fut.await);
        }
    }

    // --- Service routes (rest_get methods from metadata) ---
    if req.method() != hyper::Method::GET {
        return Ok(error_response(StatusCode::METHOD_NOT_ALLOWED, "method not allowed"));
    }

    // Convention: GET /{prefix}/{param_value} → dispatches "{prefix}.{method_name}" with param_value
    let segments: Vec<&str> = path.trim_start_matches('/').splitn(2, '/').collect();
    if segments.len() != 2 || segments[0].is_empty() || segments[1].is_empty() {
        return Ok(error_response(StatusCode::NOT_FOUND, "not found"));
    }

    let prefix = segments[0];
    let param_value = segments[1];

    // Find a rest_get method matching this prefix
    let rest_method = dispatcher.service_metas().into_iter()
        .filter(|meta| meta.prefix == prefix)
        .flat_map(|meta| meta.methods.iter())
        .find(|m| m.rest_get);

    let method_meta = match rest_method {
        Some(m) => m,
        None => return Ok(error_response(StatusCode::NOT_FOUND, "not found")),
    };

    // Dispatch: single param from the URL path
    let params = serde_json::to_value(param_value).unwrap_or_default();
    let result = dispatcher.dispatch(method_meta.name, params).await;

    match result {
        Ok(value) => {
            // If response has "data" (base64) + "mime_type" fields, serve as raw bytes
            if let (Some(data_val), Some(mime_val)) = (value.get("data"), value.get("mime_type")) {
                let b64 = data_val.as_str().unwrap_or_default();
                let mime_type = mime_val.as_str().unwrap_or("application/octet-stream");
                let data = match simply_rpc::decode_base64(b64) {
                    Ok(d) => d,
                    Err(_) => return Ok(error_response(
                        StatusCode::INTERNAL_SERVER_ERROR, "base64 decode error",
                    )),
                };

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", mime_type)
                    .header("Content-Length", data.len().to_string())
                    .header("Cache-Control", "public, max-age=31536000, immutable")
                    .header("ETag", format!("\"{param_value}\""))
                    .body(Full::new(hyper::body::Bytes::from(data)))
                    .unwrap())
            } else {
                // Return as JSON
                Ok(json_response(StatusCode::OK, &value))
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("No such file") {
                Ok(error_response(StatusCode::NOT_FOUND, &msg))
            } else {
                Ok(error_response(StatusCode::INTERNAL_SERVER_ERROR, &msg))
            }
        }
    }
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

fn error_response(status: StatusCode, message: &str) -> HyperResponse {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain")
        .body(Full::new(hyper::body::Bytes::from(message.to_string())))
        .unwrap()
}
