//! Transient per-user, per-MCP-server OAuth token store.
//!
//! Tokens live in-memory only — daemon restart clears everything.
//! GDPR-safe: nothing persisted to disk.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use simply_core::storage::ids::UserId;

/// Key for the token store: (user_id, server_id).
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct TokenKey {
    user_id: String,
    server_id: String,
}

/// A stored OAuth token.
#[derive(Clone, Debug)]
pub struct McpUserToken {
    pub access_token: String,
    pub expires_at: Option<Instant>,
    /// Display identity from the OAuth provider (e.g. email).
    pub identity: Option<String>,
}

impl McpUserToken {
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|e| Instant::now() >= e).unwrap_or(false)
    }

    pub fn expires_in_secs(&self) -> Option<u64> {
        self.expires_at.map(|e| {
            e.checked_duration_since(Instant::now())
                .map(|d| d.as_secs())
                .unwrap_or(0)
        })
    }
}

/// In-memory token store. Thread-safe.
pub struct TransientTokenStore {
    tokens: Mutex<HashMap<TokenKey, McpUserToken>>,
}

impl TransientTokenStore {
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }

    /// Store a token for (user_id, server_id).
    pub fn store(&self, user_id: &UserId, server_id: &str, token: McpUserToken) {
        let key = TokenKey {
            user_id: user_id.as_str().to_string(),
            server_id: server_id.to_string(),
        };
        self.tokens.lock().unwrap().insert(key, token);
    }

    /// Get a valid (non-expired) token for (user_id, server_id).
    pub fn get(&self, user_id: &UserId, server_id: &str) -> Option<McpUserToken> {
        let key = TokenKey {
            user_id: user_id.as_str().to_string(),
            server_id: server_id.to_string(),
        };
        let tokens = self.tokens.lock().unwrap();
        tokens.get(&key).filter(|t| !t.is_expired()).cloned()
    }

    /// Check if a valid token exists.
    pub fn has_token(&self, user_id: &UserId, server_id: &str) -> bool {
        self.get(user_id, server_id).is_some()
    }

    /// Remove a token.
    pub fn remove(&self, user_id: &UserId, server_id: &str) {
        let key = TokenKey {
            user_id: user_id.as_str().to_string(),
            server_id: server_id.to_string(),
        };
        self.tokens.lock().unwrap().remove(&key);
    }
}
