//! Auth routes — session management for the admin page.
//!
//! Auth model:
//! - Localhost requests are admin by default (no login)
//! - Non-localhost requires a session (future: passkeys)
//! - Sessions are in-memory with TTL — vanish on restart

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::response::Json;
use tokio::sync::Mutex;

/// In-memory session store. Sessions expire after TTL and vanish on restart.
#[derive(Clone)]
pub struct SessionStore {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    ttl: Duration,
}

struct Session {
    _created: Instant,
    expires: Instant,
}

impl SessionStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    /// Create a new session, returns the token.
    pub async fn create(&self) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let now = Instant::now();
        self.sessions.lock().await.insert(token.clone(), Session {
            _created: now,
            expires: now + self.ttl,
        });
        token
    }

    /// Check if a session token is valid.
    pub async fn validate(&self, token: &str) -> bool {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(token) {
            if session.expires > Instant::now() {
                return true;
            }
            sessions.remove(token);
        }
        false
    }

    /// Remove expired sessions.
    pub async fn cleanup(&self) {
        let now = Instant::now();
        self.sessions.lock().await.retain(|_, s| s.expires > now);
    }
}

/// Check if a request comes from localhost.
pub fn is_localhost(addr: Option<&std::net::SocketAddr>) -> bool {
    addr.map(|a| a.ip().is_loopback()).unwrap_or(false)
}

/// GET /auth/status — admin page checks this to decide what UI to show
pub async fn auth_status(State(_state): State<SessionStore>) -> Json<serde_json::Value> {
    let settings = config::Settings::load();
    Json(serde_json::json!({
        "auth_method": "localhost",
        "is_setup_complete": settings.user_email.is_some(),
    }))
}
