//! OAuth provider registry — loaded from `~/.local/share/noema/oauth_providers.toml`.
//!
//! This file is the single source of truth for OAuth provider identity:
//! URLs and user-configured credentials (client_id, client_secret).
//!
//! Scopes are NOT stored here — they are declared by consumers:
//! - MCP servers: scopes on `ServerAuth::OAuth`
//! - Skills: scopes on `OAuthRequirement`
//!
//! Multiple MCP servers and skills can reference the same provider_id
//! (e.g., "google") and share tokens. Removing an MCP server does not
//! remove the provider config — client_id/secret survive.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::mcp::auth::ServerAuth;

/// An OAuth provider — identity and user-configured credentials.
/// No scopes: those are declared per consumer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OAuthProvider {
    #[serde(default)]
    pub display_name: String,
    pub authorization_url: String,
    pub token_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userinfo_url: Option<String>,
    /// User-configured OAuth client ID. Empty until the user sets it.
    #[serde(default)]
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

const DEFAULT_PROVIDERS: &str = include_str!("defaults/oauth_providers.toml");

/// Create the oauth_providers.toml file with defaults if missing.
pub fn ensure_config() {
    let Some(path) = config::PathManager::oauth_providers_path() else { return };
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&path, DEFAULT_PROVIDERS).is_ok() {
            tracing::info!("Created default {}", path.display());
        }
    }
}

pub fn load_providers() -> HashMap<String, OAuthProvider> {
    let Some(path) = config::PathManager::oauth_providers_path() else { return HashMap::new() };
    let Ok(content) = std::fs::read_to_string(&path) else { return HashMap::new() };
    toml::from_str(&content).unwrap_or_default()
}

pub fn save_providers(providers: &HashMap<String, OAuthProvider>) -> anyhow::Result<()> {
    let path = config::PathManager::oauth_providers_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine oauth_providers.toml path"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(providers)?;
    std::fs::write(&path, content)?;
    Ok(())
}

pub fn lookup_provider(name: &str) -> Option<OAuthProvider> {
    load_providers().remove(name)
}

pub fn known_providers() -> Vec<String> {
    load_providers().keys().cloned().collect()
}

/// Fully resolved OAuth config — provider identity + consumer's scopes.
pub struct ResolvedOAuth {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub authorization_url: String,
    pub token_url: String,
    pub userinfo_url: Option<String>,
    pub scopes: Vec<String>,
}

/// Resolve OAuth for an MCP server: look up provider by id, combine with server's scopes.
pub fn resolve_server_auth(auth: &ServerAuth) -> Option<ResolvedOAuth> {
    let ServerAuth::OAuth { provider_id, scopes } = auth else { return None };
    let provider = lookup_provider(provider_id)?;
    Some(ResolvedOAuth {
        client_id: provider.client_id,
        client_secret: provider.client_secret,
        authorization_url: provider.authorization_url,
        token_url: provider.token_url,
        userinfo_url: provider.userinfo_url,
        scopes: scopes.clone(),
    })
}

/// Resolve OAuth for a skill: provider by id + union of skill-declared scopes.
pub fn resolve_provider_with_scopes(provider_id: &str, scopes: Vec<String>) -> Option<ResolvedOAuth> {
    let provider = lookup_provider(provider_id)?;
    Some(ResolvedOAuth {
        client_id: provider.client_id,
        client_secret: provider.client_secret,
        authorization_url: provider.authorization_url,
        token_url: provider.token_url,
        userinfo_url: provider.userinfo_url,
        scopes,
    })
}
