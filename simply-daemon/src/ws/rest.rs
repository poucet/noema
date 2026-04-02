//! Lightweight HTTP REST server for the daemon.
//!
//! Auto-routes `#[rpc(rest_get)]` methods from service metadata.
//! Also serves built-in management endpoints: `/health`, `/kill`.

use std::sync::Arc;

use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use simply_rpc::Dispatcher;

use super::server::ConnectionTracker;

/// Starts a REST HTTP server on the given port.
///
/// Returns the handle and a receiver that fires when `/kill` is called.
pub async fn start(dispatcher: Dispatcher, port: u16) -> anyhow::Result<(RestHandle, mpsc::Receiver<()>)> {
    start_with_tracker(dispatcher, port, None).await
}

pub async fn start_with_tracker(
    dispatcher: Dispatcher,
    port: u16,
    tracker: Option<ConnectionTracker>,
) -> anyhow::Result<(RestHandle, mpsc::Receiver<()>)> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(port, "REST server listening");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
    let (kill_tx, kill_rx) = mpsc::channel(1);
    let dispatcher = Arc::new(dispatcher);
    let kill_tx = Arc::new(kill_tx);
    let tracker = Arc::new(tracker);

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let dispatcher = Arc::clone(&dispatcher);
                            let kill_tx = Arc::clone(&kill_tx);
                            let tracker = Arc::clone(&tracker);
                            tokio::spawn(async move {
                                let service = service_fn(move |req| {
                                    let dispatcher = Arc::clone(&dispatcher);
                                    let kill_tx = Arc::clone(&kill_tx);
                                    let tracker = Arc::clone(&tracker);
                                    async move { handle_request(req, &dispatcher, &kill_tx, &tracker).await }
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

    Ok((RestHandle { _task: handle, _shutdown: shutdown_tx, port }, kill_rx))
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
    kill_tx: &mpsc::Sender<()>,
    tracker: &Option<ConnectionTracker>,
) -> Result<HyperResponse, std::convert::Infallible> {
    let path = req.uri().path().to_string();

    tracing::debug!(method = %req.method(), path = %path, "REST request");

    // --- Built-in management endpoints ---
    match path.as_str() {
        "/health" => {
            return Ok(json_response(StatusCode::OK, &serde_json::json!({
                "status": "ok",
            })));
        }
        "/kill" if req.method() == hyper::Method::POST => {
            tracing::info!("Kill requested via REST");
            let _ = kill_tx.send(()).await;
            return Ok(json_response(StatusCode::OK, &serde_json::json!({
                "status": "shutting_down",
            })));
        }
        "/admin" | "/admin/" => {
            return Ok(admin_page(tracker).await);
        }
        "/admin/api/connections" => {
            let connections = match tracker {
                Some(t) => t.list().await,
                None => vec![],
            };
            return Ok(json_response(StatusCode::OK, &serde_json::json!(connections)));
        }
        _ => {}
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

fn html_response(html: &str) -> HyperResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Full::new(hyper::body::Bytes::from(html.to_string())))
        .unwrap()
}

async fn admin_page(tracker: &Option<ConnectionTracker>) -> HyperResponse {
    let connections = match tracker {
        Some(t) => t.list().await,
        None => vec![],
    };

    let mut rows = String::new();
    for conn in &connections {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            conn.id, conn.addr, conn.connected_at,
        ));
    }
    if connections.is_empty() {
        rows.push_str("<tr><td colspan=\"3\" style=\"text-align:center;color:#888\">No active connections</td></tr>");
    }

    let html = format!(r#"<!DOCTYPE html>
<html>
<head>
<title>simply-daemon</title>
<meta charset="utf-8">
<style>
  body {{ font-family: -apple-system, system-ui, sans-serif; max-width: 700px; margin: 40px auto; padding: 0 20px; background: #1a1a2e; color: #e0e0e0; }}
  h1 {{ font-size: 1.4em; color: #fff; }}
  table {{ width: 100%; border-collapse: collapse; margin: 20px 0; }}
  th, td {{ text-align: left; padding: 8px 12px; border-bottom: 1px solid #333; }}
  th {{ color: #888; font-weight: 500; font-size: 0.85em; text-transform: uppercase; }}
  .count {{ color: #888; font-weight: normal; font-size: 0.9em; }}
  .actions {{ margin: 30px 0; }}
  button {{ background: #c0392b; color: #fff; border: none; padding: 10px 20px; border-radius: 4px; cursor: pointer; font-size: 0.9em; }}
  button:hover {{ background: #e74c3c; }}
  button:disabled {{ background: #555; cursor: not-allowed; }}
  #status {{ margin-top: 10px; font-size: 0.85em; color: #888; }}
</style>
</head>
<body>
<h1>simply-daemon <span class="count">({conn_count} connection{s})</span></h1>
<table>
  <thead><tr><th>ID</th><th>Address</th><th>Connected</th></tr></thead>
  <tbody>{rows}</tbody>
</table>
<div class="actions">
  <button onclick="killDaemon()" id="killBtn">Kill Daemon</button>
  <div id="status"></div>
</div>
<script>
async function killDaemon() {{
  if (!confirm('Shut down the daemon? Connected clients will need to reconnect.')) return;
  const btn = document.getElementById('killBtn');
  const status = document.getElementById('status');
  btn.disabled = true;
  btn.textContent = 'Shutting down...';
  try {{
    await fetch('/kill', {{ method: 'POST' }});
    status.textContent = 'Shutdown signal sent.';
  }} catch (e) {{
    status.textContent = 'Request sent (daemon may already be down).';
  }}
}}
</script>
</body>
</html>"#,
        conn_count = connections.len(),
        s = if connections.len() == 1 { "" } else { "s" },
        rows = rows,
    );

    html_response(&html)
}
