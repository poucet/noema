//! OAuth callback state tracking.
//!
//! No longer runs a separate server — the daemon's main HTTP server
//! handles the callback at `/auth/mcp/callback`. This module just
//! tracks pending flows and dispatches results.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

/// A callback result: (authorization_code, state).
type CallbackResult = Result<(String, String), String>;

/// Tracks pending OAuth flows. The daemon's axum route calls `dispatch`
/// when the callback arrives.
pub struct CallbackTracker {
    public_url: String,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<CallbackResult>>>>,
}

impl CallbackTracker {
    /// Create a tracker. `public_url` is the daemon's externally-reachable base URL
    /// (e.g. "http://localhost:9800" or "https://my-server.com/daemon").
    pub fn new(public_url: String) -> Self {
        Self {
            public_url,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The redirect URI — derived from the daemon's public URL.
    pub fn redirect_uri(&self) -> String {
        format!("{}/auth/mcp/callback", self.public_url.trim_end_matches('/'))
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

    /// Dispatch an OAuth callback result. Called by the axum route handler.
    /// Returns true if a pending flow was found and notified.
    pub async fn dispatch(&self, state: &str, code: &str) -> bool {
        if let Some(tx) = self.pending.lock().await.remove(state) {
            let _ = tx.send(Ok((code.to_string(), state.to_string())));
            true
        } else {
            tracing::warn!(state, "OAuth callback for unknown state");
            false
        }
    }

    /// Dispatch an error for a pending flow.
    pub async fn dispatch_error(&self, state: &str, error: &str) -> bool {
        if let Some(tx) = self.pending.lock().await.remove(state) {
            let _ = tx.send(Err(error.to_string()));
            true
        } else {
            false
        }
    }
}
