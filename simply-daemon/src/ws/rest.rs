//! Lightweight HTTP REST server for the daemon.
//!
//! Auto-routes requests to services via `RestDispatcher`.
//! Also supports legacy `rest_get` methods via `Dispatcher` metadata,
//! and optional extra route handler (admin dashboard).

use std::sync::Arc;

use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use simply_rpc::{Dispatcher, HttpMethod, RestDispatcher};

use crate::admin::RouteHandler;

/// REST server configuration.
pub struct RestConfig {
    pub dispatcher: Dispatcher,
    pub rest_dispatcher: RestDispatcher,
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
    let rest_dispatcher = Arc::new(config.rest_dispatcher);
    let extra_routes: Arc<Option<RouteHandler>> = Arc::new(config.extra_routes);

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let dispatcher = Arc::clone(&dispatcher);
                            let rest_dispatcher = Arc::clone(&rest_dispatcher);
                            let extra_routes = Arc::clone(&extra_routes);
                            tokio::spawn(async move {
                                let service = service_fn(move |req| {
                                    let dispatcher = Arc::clone(&dispatcher);
                                    let rest_dispatcher = Arc::clone(&rest_dispatcher);
                                    let extra_routes = Arc::clone(&extra_routes);
                                    async move { handle_request(req, &dispatcher, &rest_dispatcher, &extra_routes).await }
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
    rest_dispatcher: &RestDispatcher,
    extra_routes: &Option<RouteHandler>,
) -> Result<HyperResponse, std::convert::Infallible> {
    let path = req.uri().path().to_string();
    let method_str = req.method().to_string();

    tracing::debug!(method = %method_str, path = %path, "REST request");

    // --- Extra routes (admin dashboard, static pages) ---
    if let Some(handler) = extra_routes {
        if let Some(fut) = handler(&path, &method_str) {
            return Ok(fut.await);
        }
    }

    // --- REST-annotated service routes (new system) ---
    let http_method = match method_str.as_str() {
        "GET" => Some(HttpMethod::Get),
        "POST" => Some(HttpMethod::Post),
        "PUT" => Some(HttpMethod::Put),
        "DELETE" => Some(HttpMethod::Delete),
        _ => None,
    };

    if let Some(http_method) = http_method {
        // Read body for POST/PUT
        let body = match http_method {
            HttpMethod::Post | HttpMethod::Put => {
                use http_body_util::BodyExt;
                match req.collect().await {
                    Ok(collected) => {
                        let bytes = collected.to_bytes();
                        if bytes.is_empty() {
                            serde_json::Value::Null
                        } else {
                            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
                        }
                    }
                    Err(_) => serde_json::Value::Null,
                }
            }
            _ => serde_json::Value::Null,
        };

        if let Some(result) = rest_dispatcher.dispatch(http_method, &path, body).await {
            return Ok(match result {
                Ok(value) => {
                    // If response has "data" (base64) + "mime_type" fields, serve as raw bytes
                    if let (Some(data_val), Some(mime_val)) = (value.get("data"), value.get("mime_type")) {
                        let b64 = data_val.as_str().unwrap_or_default();
                        let mime_type = mime_val.as_str().unwrap_or("application/octet-stream");
                        match simply_rpc::decode_base64(b64) {
                            Ok(data) => {
                                let param_value = path.rsplit('/').next().unwrap_or("");
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .header("Content-Type", mime_type)
                                    .header("Content-Length", data.len().to_string())
                                    .header("Cache-Control", "public, max-age=31536000, immutable")
                                    .header("ETag", format!("\"{param_value}\""))
                                    .body(Full::new(hyper::body::Bytes::from(data)))
                                    .unwrap()
                            }
                            Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "base64 decode error"),
                        }
                    } else {
                        json_response(StatusCode::OK, &value)
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("not found") || msg.contains("No such file") {
                        error_response(StatusCode::NOT_FOUND, &msg)
                    } else {
                        error_response(StatusCode::INTERNAL_SERVER_ERROR, &msg)
                    }
                }
            });
        }
    }

    // --- Legacy rest_get routes (for backward compat during migration) ---
    if method_str == "GET" {
        let segments: Vec<&str> = path.trim_start_matches('/').splitn(2, '/').collect();
        if segments.len() == 2 && !segments[0].is_empty() && !segments[1].is_empty() {
            let prefix = segments[0];
            let param_value = segments[1];

            let rest_method = dispatcher.service_metas().into_iter()
                .filter(|meta| meta.prefix == prefix)
                .flat_map(|meta| meta.methods.iter())
                .find(|m| m.rest_get);

            if let Some(method_meta) = rest_method {
                let params = serde_json::to_value(param_value).unwrap_or_default();
                let result = dispatcher.dispatch(method_meta.name, params).await;

                return Ok(match result {
                    Ok(value) => {
                        if let (Some(data_val), Some(mime_val)) = (value.get("data"), value.get("mime_type")) {
                            let b64 = data_val.as_str().unwrap_or_default();
                            let mime_type = mime_val.as_str().unwrap_or("application/octet-stream");
                            match simply_rpc::decode_base64(b64) {
                                Ok(data) => Response::builder()
                                    .status(StatusCode::OK)
                                    .header("Content-Type", mime_type)
                                    .header("Content-Length", data.len().to_string())
                                    .header("Cache-Control", "public, max-age=31536000, immutable")
                                    .header("ETag", format!("\"{param_value}\""))
                                    .body(Full::new(hyper::body::Bytes::from(data)))
                                    .unwrap(),
                                Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "base64 decode error"),
                            }
                        } else {
                            json_response(StatusCode::OK, &value)
                        }
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("not found") || msg.contains("No such file") {
                            error_response(StatusCode::NOT_FOUND, &msg)
                        } else {
                            error_response(StatusCode::INTERNAL_SERVER_ERROR, &msg)
                        }
                    }
                });
            }
        }
    }

    Ok(error_response(StatusCode::NOT_FOUND, "not found"))
}

fn json_response(status: StatusCode, value: &serde_json::Value) -> HyperResponse {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
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
