//! Built-in OAuth provider defaults for well-known services.
//!
//! When a ServerConfig has `oauth_provider = "google"` + `client_id`,
//! the daemon resolves the full OAuth config (URLs, scopes) from this table.

use simply_core::AuthMethod;

/// Built-in OAuth provider defaults.
pub struct OAuthProviderDefaults {
    pub authorization_url: &'static str,
    pub token_url: &'static str,
    pub scopes: &'static [&'static str],
}

impl OAuthProviderDefaults {
    pub fn lookup(provider: &str) -> Option<Self> {
        match provider {
            "google" => Some(Self {
                authorization_url: "https://accounts.google.com/o/oauth2/v2/auth",
                token_url: "https://oauth2.googleapis.com/token",
                scopes: &[
                    "https://www.googleapis.com/auth/drive.readonly",
                    "https://www.googleapis.com/auth/documents.readonly",
                ],
            }),
            "github" => Some(Self {
                authorization_url: "https://github.com/login/oauth/authorize",
                token_url: "https://github.com/login/oauth/access_token",
                scopes: &["repo", "read:user"],
            }),
            "notion" => Some(Self {
                authorization_url: "https://api.notion.com/v1/oauth/authorize",
                token_url: "https://api.notion.com/v1/oauth/token",
                scopes: &[],
            }),
            _ => None,
        }
    }

    pub fn known_providers() -> &'static [&'static str] {
        &["google", "github", "notion"]
    }
}

/// Resolve OAuth config for a server, using built-in provider defaults.
///
/// If the server has `oauth_provider` + `client_id`, expands into full OAuth.
/// Otherwise falls back to the server's `auth` field.
pub fn resolve_server_auth(config: &simply_core::ServerConfig) -> Option<ResolvedOAuth> {
    let provider = config.oauth_provider.as_deref()?;
    let client_id = config.client_id.as_deref()?;
    let defaults = OAuthProviderDefaults::lookup(provider)?;

    Some(ResolvedOAuth {
        client_id: client_id.to_string(),
        client_secret: config.client_secret.clone(),
        authorization_url: defaults.authorization_url.to_string(),
        token_url: defaults.token_url.to_string(),
        scopes: defaults.scopes.iter().map(|s| s.to_string()).collect(),
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
