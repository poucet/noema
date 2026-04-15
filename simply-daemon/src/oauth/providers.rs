//! OAuth provider defaults loaded from ~/.config/noema/oauth_providers.toml.
//!
//! When a ServerConfig has `oauth_provider = "google"` + `client_id`,
//! the daemon resolves the full OAuth config (URLs, scopes) from this file.
//! Adding a new provider is just editing the TOML — no recompilation.

use serde::Deserialize;
use std::collections::HashMap;

/// Provider defaults from oauth_providers.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProviderDefaults {
    pub authorization_url: String,
    pub token_url: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Default oauth_providers.toml content, compiled in for first-run copy.
const DEFAULT_PROVIDERS: &str = include_str!("defaults/oauth_providers.toml");

/// Ensure oauth_providers.toml exists in the config dir.
/// Copies the default if missing.
pub fn ensure_config() {
    let Some(dir) = config::PathManager::config_dir() else { return };
    let path = dir.join("oauth_providers.toml");
    if !path.exists() {
        let _ = std::fs::create_dir_all(&dir);
        if std::fs::write(&path, DEFAULT_PROVIDERS).is_ok() {
            tracing::info!("Created default {}", path.display());
        }
    }
}

/// Load all provider defaults from ~/.config/noema/oauth_providers.toml.
fn load_providers() -> HashMap<String, OAuthProviderDefaults> {
    let Some(dir) = config::PathManager::config_dir() else {
        return HashMap::new();
    };
    let path = dir.join("oauth_providers.toml");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// Look up a provider by name.
pub fn lookup_provider(name: &str) -> Option<OAuthProviderDefaults> {
    load_providers().remove(name)
}

/// List all known provider names.
pub fn known_providers() -> Vec<String> {
    load_providers().keys().cloned().collect()
}

/// Resolve OAuth config for a server, using provider defaults.
pub fn resolve_server_auth(config: &simply_core::ServerConfig) -> Option<ResolvedOAuth> {
    let provider = config.oauth_provider.as_deref()?;
    let client_id = config.client_id.as_deref()?;
    let defaults = lookup_provider(provider)?;

    Some(ResolvedOAuth {
        client_id: client_id.to_string(),
        client_secret: config.client_secret.clone(),
        authorization_url: defaults.authorization_url,
        token_url: defaults.token_url,
        scopes: defaults.scopes,
    })
}

/// Fully resolved OAuth config.
pub struct ResolvedOAuth {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub authorization_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
}
