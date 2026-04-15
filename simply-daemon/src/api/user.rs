//! User identity API — resolve and create users from external identities.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Resolved user identity.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserIdentity {
    pub user_id: String,
    pub email: Option<String>,
}

#[simply_rpc::rpc_service("user")]
#[async_trait]
pub trait UserApi: Send + Sync {
    /// Resolve an external identity to a daemon user.
    /// Creates a new user if one doesn't exist for this identity.
    /// The external_id is an opaque string (e.g. "discord:123456789").
    #[rpc(post = "/user/resolve/{external_id}", no_tool)]
    async fn resolve_or_create_user(&self, external_id: String) -> anyhow::Result<UserIdentity>;
}
