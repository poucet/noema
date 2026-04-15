//! User identity API — resolve and create users from external identities.

use async_trait::async_trait;
use simply_rpc::{RequestContext, Scope};

#[simply_rpc::rpc_service("user")]
#[async_trait]
pub trait UserApi: Send + Sync {
    /// Resolve an external identity to a daemon user.
    /// Creates a new user if one doesn't exist for this identity.
    /// The external_id is an opaque string (e.g. "discord:123456789").
    /// Returns a Scope that can be used for subsequent requests.
    #[rpc(post = "/user/resolve/{external_id}", no_tool)]
    async fn resolve_or_create_user(&self, ctx: RequestContext, external_id: String) -> anyhow::Result<Scope>;
}
