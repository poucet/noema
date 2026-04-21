//! User identity API — resolve and create users from external identities.

use async_trait::async_trait;
use simply_rpc::{RequestContext, Scope};

#[simply_rpc::rpc_service("user")]
#[async_trait]
pub trait UserApi: Send + Sync {
    /// Look up an external identity. Returns the user's Scope if mapped, None if unknown.
    /// Does NOT create a user — use `resolve_or_create_user` for that.
    #[rpc(get = "/user/resolve/{external_id}", no_tool)]
    async fn resolve_user(&self, ctx: &RequestContext, external_id: String) -> anyhow::Result<Option<Scope>>;

    /// Resolve an external identity to a daemon user.
    /// Creates a new user if one doesn't exist for this identity.
    /// The external_id is an opaque string (e.g. "discord:123456789").
    /// Returns a Scope that can be used for subsequent requests.
    #[rpc(post = "/user/resolve/{external_id}", no_tool)]
    async fn resolve_or_create_user(&self, ctx: &RequestContext, external_id: String) -> anyhow::Result<Scope>;

    /// Look up a user by email, creating one if missing. Matches admin's
    /// "Create user" flow (same underlying `user_store.get_or_create_user_by_email`),
    /// so single-user desktop clients like Noema converge on the same user id
    /// admin ends up showing in the picker.
    #[rpc(post = "/user/by-email", no_tool)]
    async fn get_or_create_user_by_email(&self, ctx: &RequestContext, email: String) -> anyhow::Result<Scope>;
}
