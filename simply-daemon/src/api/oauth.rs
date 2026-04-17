//! OAuth flow management for MCP servers.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Information returned when starting an OAuth flow.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "admin/src/lib/generated/types/")]
pub struct OAuthFlowInfo {
    /// The authorization URL the user should open in their browser.
    pub auth_url: String,
    /// The state parameter — must be passed back in `complete_oauth`.
    pub state: String,
}

#[simply_rpc::rpc_service("oauth")]
#[async_trait]
pub trait OAuthApi: Send + Sync {
    /// Start an OAuth flow for an MCP server.
    #[rpc(post = "/oauth/{server_id}")]
    async fn start_oauth(&self, server_id: &str) -> anyhow::Result<OAuthFlowInfo>;

    /// Complete an OAuth flow by exchanging an authorization code for tokens.
    #[rpc(post = "/oauth/{server_id}/complete")]
    async fn complete_oauth(&self, server_id: &str, code: &str, state: &str) -> anyhow::Result<()>;

    /// Complete an OAuth flow using just a code (manual entry, no state verification).
    #[rpc(post = "/oauth/{server_id}/code")]
    async fn complete_oauth_with_code(&self, server_id: &str, code: &str) -> anyhow::Result<()>;

    /// Look up which server ID a pending OAuth state parameter belongs to.
    #[rpc(get = "/oauth/{state}")]
    async fn resolve_oauth_state(&self, state: &str) -> Option<String>;
}
