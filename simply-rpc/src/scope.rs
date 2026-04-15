//! Request scope — per-request context carrying user identity.
//!
//! Every service method can optionally receive a `Scope` parameter.
//! The RPC dispatch layer injects it from the request context.
//! Anonymous requests get `Scope::anonymous()`.

use serde::{Deserialize, Serialize};

/// Per-request scope carrying the calling user's identity.
///
/// Lightweight value — cheap to create and clone. The expensive work
/// (resolving per-user MCP connections, etc.) only happens when services
/// need it and `user_id` is `Some`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Scope {
    /// None = anonymous/default, Some = authenticated user.
    pub user_id: Option<String>,
}

impl Scope {
    /// Create an anonymous scope (no user identity).
    pub fn anonymous() -> Self {
        Self { user_id: None }
    }

    /// Create a scope for an authenticated user.
    pub fn user(user_id: impl Into<String>) -> Self {
        Self {
            user_id: Some(user_id.into()),
        }
    }

    /// Check if this scope has an authenticated user.
    pub fn is_authenticated(&self) -> bool {
        self.user_id.is_some()
    }
}
