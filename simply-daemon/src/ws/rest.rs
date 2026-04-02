//! Lightweight HTTP REST server for the daemon.
//!
//! Runs alongside the WS server. Currently serves blob assets as
//! HTTP GET with proper caching headers. Future: more REST endpoints.

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

/// Starts a REST HTTP server on the given port.
pub async fn start(dispatcher: Dispatcher, port: u16) -> anyhow::Result<RestHandle> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(port, "REST server listening");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
    let dispatcher = Arc::new(dispatcher);

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let dispatcher = Arc::clone(&dispatcher);
                            tokio::spawn(async move {
                                let service = service_fn(move |req| {
                                    let dispatcher = Arc::clone(&dispatcher);
                                    async move { handle_request(req, &dispatcher).await }
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

    Ok(RestHandle { _task: handle, _shutdown: shutdown_tx, port })
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
) -> Result<HyperResponse, std::convert::Infallible> {
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|q| q.to_string());

    tracing::debug!(method = %req.method(), path = %path, "REST request");

    // Route: GET /asset/{hash}?mime_type=X
    if req.method() == hyper::Method::GET && path.starts_with("/asset/") {
        let hash = &path["/asset/".len()..];
        return Ok(handle_get_blob(hash, query.as_deref(), dispatcher).await);
    }

    // Route: POST /rpc/{method} — generic RPC over REST
    if req.method() == hyper::Method::POST && path.starts_with("/rpc/") {
        let method = &path["/rpc/".len()..];
        let body = match http_body_util::BodyExt::collect(req.into_body()).await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return Ok(error_response(StatusCode::BAD_REQUEST, "invalid body")),
        };
        let params: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
        let result = dispatcher.dispatch(method, params).await;
        return Ok(rpc_response(result));
    }

    // 404 for everything else
    Ok(error_response(StatusCode::NOT_FOUND, "not found"))
}

async fn handle_get_blob(
    hash: &str,
    query: Option<&str>,
    dispatcher: &Dispatcher,
) -> HyperResponse {
    let params = serde_json::to_value(hash).unwrap_or_default();
    let result = dispatcher.dispatch("asset.get_blob", params).await;

    match result {
        Ok(value) => {
            // The value is a base64-encoded string (from #[rpc(base64_return)])
            let b64: String = match serde_json::from_value(value) {
                Ok(s) => s,
                Err(_) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, "invalid blob data"),
            };
            let data = match simply_rpc::decode_base64(&b64) {
                Ok(d) => d,
                Err(_) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, "base64 decode error"),
            };

            let mime_type = query
                .and_then(|q| {
                    q.split('&')
                        .find(|p| p.starts_with("mime_type="))
                        .map(|p| urlencoding::decode(p.trim_start_matches("mime_type="))
                            .unwrap_or_default()
                            .into_owned())
                })
                .unwrap_or_else(|| "application/octet-stream".to_string());

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime_type)
                .header("Content-Length", data.len().to_string())
                .header("Cache-Control", "public, max-age=31536000, immutable")
                .header("ETag", format!("\"{hash}\""))
                .body(Full::new(hyper::body::Bytes::from(data)))
                .unwrap()
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("No such file") {
                error_response(StatusCode::NOT_FOUND, &format!("blob not found: {hash}"))
            } else {
                error_response(StatusCode::INTERNAL_SERVER_ERROR, &msg)
            }
        }
    }
}

fn rpc_response(result: simply_rpc::RpcResult) -> HyperResponse {
    match result {
        Ok(value) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Full::new(hyper::body::Bytes::from(
                serde_json::to_vec(&value).unwrap_or_default(),
            )))
            .unwrap(),
        Err(e) => {
            let body = serde_json::json!({ "error": e.to_string() });
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "application/json")
                .body(Full::new(hyper::body::Bytes::from(
                    serde_json::to_vec(&body).unwrap_or_default(),
                )))
                .unwrap()
        }
    }
}

fn error_response(status: StatusCode, message: &str) -> HyperResponse {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain")
        .body(Full::new(hyper::body::Bytes::from(message.to_string())))
        .unwrap()
}
