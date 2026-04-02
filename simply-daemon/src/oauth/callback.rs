//! Long-lived OAuth callback server.
//!
//! Runs on a stable configured port for the lifetime of the daemon.
//! Multiple concurrent OAuth flows are multiplexed by the `state` query param.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};

/// A callback result: (authorization_code, state).
type CallbackResult = Result<(String, String), String>;

/// Long-lived OAuth callback server.
/// Call `register` before starting a flow, then `await` the returned receiver.
pub struct CallbackServer {
    port: u16,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<CallbackResult>>>>,
    _shutdown_tx: oneshot::Sender<()>,
}

const DEFAULT_PORT: u16 = 9876;

impl CallbackServer {
    /// Start the callback server on the given port (or default 9876).
    pub async fn start(port: Option<u16>) -> anyhow::Result<Self> {
        let port = port.unwrap_or(DEFAULT_PORT);
        let addr: SocketAddr = ([127, 0, 0, 1], port).into();
        let listener = TcpListener::bind(addr).await?;
        let actual_port = listener.local_addr()?.port();

        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<CallbackResult>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let pending_clone = Arc::clone(&pending);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    result = listener.accept() => {
                        match result {
                            Ok((stream, _)) => {
                                let pending = Arc::clone(&pending_clone);
                                tokio::spawn(async move {
                                    handle_request(stream, pending).await;
                                });
                            }
                            Err(e) => {
                                tracing::error!("OAuth callback server accept error: {}", e);
                            }
                        }
                    }
                }
            }
        });

        tracing::info!(port = actual_port, "OAuth callback server started");

        Ok(Self {
            port: actual_port,
            pending,
            _shutdown_tx: shutdown_tx,
        })
    }

    /// The redirect URI clients should use.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Register a pending OAuth flow. Returns a receiver that resolves
    /// when the callback arrives with the matching `state` param.
    pub async fn register(&self, state: &str) -> oneshot::Receiver<CallbackResult> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(state.to_string(), tx);
        rx
    }

    /// Cancel a pending flow (e.g., on timeout).
    pub async fn cancel(&self, state: &str) {
        self.pending.lock().await.remove(state);
    }
}

async fn handle_request(
    mut stream: tokio::net::TcpStream,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<CallbackResult>>>>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();

    if reader.read_line(&mut request_line).await.is_err() {
        return;
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let path = parts[1];

    // Parse query params
    let (code, state, error) = parse_query(path);

    let result = if let Some(err) = error {
        Err(format!("OAuth error: {}", err))
    } else if let (Some(code), Some(ref state)) = (&code, &state) {
        Ok((code.clone(), state.clone()))
    } else {
        Err("Missing code or state parameter".to_string())
    };

    // Dispatch to the waiting flow by state param
    let dispatched = if let Some(ref state_val) = state {
        if let Some(tx) = pending.lock().await.remove(state_val) {
            let _ = tx.send(result);
            true
        } else {
            tracing::warn!(state = %state_val, "OAuth callback for unknown state");
            false
        }
    } else {
        false
    };

    // Respond to browser
    let (status, body) = if dispatched {
        (
            "200 OK",
            concat!(
                "<!DOCTYPE html><html><head><title>OAuth Complete</title></head>",
                "<body style=\"font-family:system-ui;display:flex;justify-content:center;",
                "align-items:center;height:100vh;margin:0;background:#1a1a1a;color:#fff\">",
                "<div style=\"text-align:center\">",
                "<h1 style=\"color:#14b8a6\">Authentication Successful</h1>",
                "<p>You can close this window.</p>",
                "</div></body></html>"
            ),
        )
    } else {
        (
            "400 Bad Request",
            concat!(
                "<!DOCTYPE html><html><head><title>OAuth Failed</title></head>",
                "<body style=\"font-family:system-ui;display:flex;justify-content:center;",
                "align-items:center;height:100vh;margin:0;background:#1a1a1a;color:#fff\">",
                "<div style=\"text-align:center\">",
                "<h1 style=\"color:#ef4444\">Authentication Failed</h1>",
                "<p>Please close this window and try again.</p>",
                "</div></body></html>"
            ),
        )
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        body.len(),
        body
    );

    let _ = stream.write_all(response.as_bytes()).await;
}

fn parse_query(path: &str) -> (Option<String>, Option<String>, Option<String>) {
    let Some(query_start) = path.find('?') else {
        return (None, None, None);
    };

    let query = &path[query_start + 1..];
    let mut code = None;
    let mut state = None;
    let mut error = None;

    for param in query.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            let decoded = urlencoding::decode(value).unwrap_or_default().to_string();
            match key {
                "code" => code = Some(decoded),
                "state" => state = Some(decoded),
                "error" => error = Some(decoded),
                _ => {}
            }
        }
    }

    (code, state, error)
}
