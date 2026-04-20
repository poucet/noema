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

/// An OAuth provider entry — for admin UI listing.
/// Never exposes `client_secret` over the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "../admin/src/lib/generated/types/"))]
pub struct OAuthProviderInfo {
    pub id: String,
    pub display_name: String,
    pub authorization_url: String,
    pub token_url: String,
    pub userinfo_url: Option<String>,
    pub client_id: String,
    /// True if client_secret is set (the secret itself is never returned).
    pub has_client_secret: bool,
    /// Last 4 characters of the client_secret, for visual confirmation. Empty if unset.
    pub client_secret_suffix: String,
    /// Scopes accumulated from skill registrations (in-memory, not persisted).
    pub registered_scopes: Vec<String>,
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

    /// List all OAuth providers configured in oauth_providers.toml.
    #[rpc(get = "/oauth/providers", no_tool)]
    async fn list_oauth_providers(&self) -> anyhow::Result<Vec<OAuthProviderInfo>>;

    /// Upsert an OAuth provider in oauth_providers.toml.
    /// This is the admin-UI entry point — set credentials + URLs.
    /// Skills also call this (with scopes) to accumulate the in-memory scope union.
    #[rpc(put = "/oauth/providers/{provider_id}", no_tool)]
    async fn register_oauth_provider(
        &self,
        provider_id: &str,
        request: RegisterOAuthProviderRequest,
    ) -> anyhow::Result<()>;

    /// Remove an OAuth provider from oauth_providers.toml.
    /// Fails if any MCP server still references this provider.
    #[rpc(delete = "/oauth/providers/{provider_id}", no_tool)]
    async fn remove_oauth_provider(&self, provider_id: &str) -> anyhow::Result<()>;
}
