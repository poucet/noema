//! OAuth callback server for loopback redirect
//!
//! Starts a temporary local HTTP server to receive OAuth callbacks
//! and capture the authorization code.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};

/// State for an active OAuth callback server
pub struct OAuthCallbackServer {
    port: u16,
    code_rx: oneshot::Receiver<Result<(String, String), String>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl OAuthCallbackServer {
    /// Get the redirect URI for this callback server
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port)
    }

    /// Get the port the server is running on
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Wait for the OAuth callback and return (code, state)
    pub async fn wait_for_callback(self) -> Result<(String, String), String> {
        self.code_rx.await.map_err(|_| "Callback cancelled".to_string())?
    }

    /// Shutdown the server without waiting for a callback
    pub fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Start a temporary OAuth callback server
///
/// Returns a server handle that can be used to get the redirect URI
/// and wait for the callback.
pub async fn start_callback_server() -> Result<OAuthCallbackServer, String> {
    // Bind to a random port on localhost
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Failed to bind callback server: {}", e))?;

    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local address: {}", e))?
        .port();

    let (code_tx, code_rx) = oneshot::channel();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

    // Wrap the sender in Arc<Mutex> so we can move it into the async block
    let code_tx = Arc::new(Mutex::new(Some(code_tx)));

    // Spawn the server task
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    break;
                }
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let code_tx = code_tx.clone();
                            tokio::spawn(async move {
                                handle_callback(stream, code_tx).await;
                            });
                        }
                        Err(e) => {
                            tracing::error!("Failed to accept connection: {}", e);
                        }
                    }
                }
            }
        }
    });

    Ok(OAuthCallbackServer {
        port,
        code_rx,
        shutdown_tx: Some(shutdown_tx),
    })
}

/// Handle an incoming HTTP request on the callback server
async fn handle_callback(
    mut stream: tokio::net::TcpStream,
    code_tx: Arc<Mutex<Option<oneshot::Sender<Result<(String, String), String>>>>>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();

    // Read the request line
    if reader.read_line(&mut request_line).await.is_err() {
        return;
    }

    // Parse the request
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let path = parts[1];

    // Parse query parameters
    let (code, state, error) = if let Some(query_start) = path.find('?') {
        let query = &path[query_start + 1..];
        let mut code = None;
        let mut state = None;
        let mut error = None;

        for param in query.split('&') {
            if let Some((key, value)) = param.split_once('=') {
                match key {
                    "code" => code = Some(urlencoding::decode(value).unwrap_or_default().to_string()),
                    "state" => state = Some(urlencoding::decode(value).unwrap_or_default().to_string()),
                    "error" => error = Some(urlencoding::decode(value).unwrap_or_default().to_string()),
                    _ => {}
                }
            }
        }

        (code, state, error)
    } else {
        (None, None, None)
    };

    // Send the result
    let result = if let Some(err) = error {
        Err(format!("OAuth error: {}", err))
    } else if let (Some(code), Some(state)) = (code, state) {
        Ok((code, state))
    } else {
        Err("Missing code or state parameter".to_string())
    };

    let is_success = result.is_ok();

    // Send result through channel
    if let Some(tx) = code_tx.lock().await.take() {
        let _ = tx.send(result);
    }

    // Send HTTP response
    let (status, body) = if is_success {
        (
            "200 OK",
            r#"<!DOCTYPE html>
<html>
<head><title>OAuth Complete</title></head>
<body style="font-family: system-ui; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #1a1a1a; color: #fff;">
<div style="text-align: center;">
<h1 style="color: #14b8a6;">Authentication Successful</h1>
<p>You can close this window and return to Noema.</p>
</div>
</body>
</html>"#,
        )
    } else {
        (
            "400 Bad Request",
            r#"<!DOCTYPE html>
<html>
<head><title>OAuth Failed</title></head>
<body style="font-family: system-ui; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #1a1a1a; color: #fff;">
<div style="text-align: center;">
<h1 style="color: #ef4444;">Authentication Failed</h1>
<p>Please close this window and try again.</p>
</div>
</body>
</html>"#,
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
