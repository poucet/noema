//! OAuth flow management — for MCP servers and skill OAuth providers.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
#[cfg(feature = "ts")]
use ts_rs::TS;

/// Information returned when starting an OAuth flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "../admin/src/lib/generated/types/"))]
pub struct OAuthFlowInfo {
    /// The authorization URL the user should open in their browser.
    pub auth_url: String,
    /// The state parameter — must be passed back in `complete_oauth`.
    pub state: String,
}

/// Request to upsert an OAuth provider config in `oauth_providers.toml`,
/// and optionally declare skill-required scopes.
///
/// - Admin UI calls: set credentials + URLs, leave `scopes` empty.
/// - Skills call: set URLs + `scopes`, leave `client_id` empty (user configures separately).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegisterOAuthProviderRequest {
    pub display_name: String,
    pub authorization_url: String,
    pub token_url: String,
    /// Scopes declared by the registering skill (merged into in-memory union).
    /// Admin UI callers should leave this empty.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Empty string means "leave existing credentials unchanged".
    #[serde(default)]
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userinfo_url: Option<String>,
}

#[simply_rpc::rpc_service("oauth")]
#[async_trait]
pub trait OAuthApi: Send + Sync {
    /// Start an OAuth flow for an MCP server or skill OAuth provider.
    #[rpc(post = "/oauth/{server_id}")]
    async fn start_oauth(&self, server_id: &str) -> anyhow::Result<OAuthFlowInfo>;

    /// Complete an OAuth flow by exchanging an authorization code for tokens.
    #[rpc(post = "/oauth/{server_id}/complete")]
    async fn complete_oauth(&self, server_id: &str, code: &str, state: &str) -> anyhow::Result<()>;

    /// Complete an OAuth flow using just a code (manual entry, no state verification).
    #[rpc(post = "/oauth/{server_id}/code")]
    async fn complete_oauth_with_code(&self, server_id: &str, code: &str) -> anyhow::Result<()>;

    /// Look up which server/provider ID a pending OAuth state parameter belongs to.
    #[rpc(get = "/oauth/{state}")]
    async fn resolve_oauth_state(&self, state: &str) -> Option<String>;

    /// Register an OAuth provider for a skill (or any non-MCP OAuth consumer).
    ///
    /// Stored in `oauth_providers` section of config — never creates a phantom
    /// MCP server entry or triggers auto-connect. The `/auth/mcp/{provider_id}`
    /// route resolves OAuth config from here.
    #[rpc(put = "/oauth/providers/{provider_id}", no_tool)]
    async fn register_oauth_provider(
        &self,
        provider_id: &str,
        request: RegisterOAuthProviderRequest,
    ) -> anyhow::Result<()>;
}
