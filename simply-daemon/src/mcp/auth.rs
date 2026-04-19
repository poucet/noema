//! Daemon-side MCP auth config.
//!
//! `simply-core` is auth-agnostic — it just manages connections given a bearer token.
//! All OAuth, credential storage, and provider config lives here.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Authentication config for an MCP server.
///
/// OAuth auth references a provider (from `oauth_providers.toml`) by ID and
/// declares server-specific scopes. Client credentials live in the provider
/// config, NOT here — so removing an MCP server never nukes your client_id.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerAuth {
    #[default]
    None,
    /// Static bearer token (e.g. a shared API key). Global connection.
    Token {
        token: String,
    },
    /// OAuth 2.0 — references a provider by ID + declares server-specific scopes.
    /// Per-user tokens in `TransientTokenStore`.
    OAuth {
        /// Name of the OAuth provider (e.g., "google"). Must exist in oauth_providers.toml.
        provider_id: String,
        /// Scopes this server needs. Unioned with other consumers of the same provider.
        #[serde(default)]
        scopes: Vec<String>,
    },
}

impl ServerAuth {
    /// Bearer token for global connections (Token variant only).
    pub fn static_bearer_token(&self) -> Option<&str> {
        match self {
            ServerAuth::Token { token } => Some(token),
            _ => None,
        }
    }

    /// Whether this server uses per-user OAuth (no global connection).
    pub fn is_oauth(&self) -> bool {
        matches!(self, ServerAuth::OAuth { .. })
    }

    /// Provider ID for OAuth servers, else None.
    pub fn oauth_provider_id(&self) -> Option<&str> {
        match self {
            ServerAuth::OAuth { provider_id, .. } => Some(provider_id),
            _ => None,
        }
    }

    /// Scopes declared by this server's OAuth config.
    pub fn oauth_scopes(&self) -> &[String] {
        match self {
            ServerAuth::OAuth { scopes, .. } => scopes,
            _ => &[],
        }
    }
}

/// Per-server config as stored in mcp.toml.
///
/// Combines the core connection config (`name`, `url`, `auto_connect`, `auto_retry`)
/// with daemon-owned auth config (`auth`). At runtime, `core()` extracts the
/// `simply_core::ServerConfig` passed to `McpRegistry`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonServerConfig {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub auth: ServerAuth,
    #[serde(default = "default_true")]
    pub auto_connect: bool,
    #[serde(default = "default_true")]
    pub auto_retry: bool,
}

fn default_true() -> bool { true }

impl DaemonServerConfig {
    /// Extract the core connection config for `McpRegistry`.
    pub fn core(&self) -> simply_core::ServerConfig {
        simply_core::ServerConfig {
            name: self.name.clone(),
            url: self.url.clone(),
            auto_connect: self.auto_connect,
            auto_retry: self.auto_retry,
        }
    }

    /// Static bearer token for global connections (Token auth only).
    pub fn static_bearer_token(&self) -> Option<&str> {
        self.auth.static_bearer_token()
    }

    /// Whether this server connects globally at startup.
    pub fn should_global_connect(&self) -> bool {
        !self.auth.is_oauth()
    }
}

/// MCP servers config — stored in mcp.toml.
///
/// OAuth provider configs (client_id, client_secret, URLs) live in a separate file
/// (`oauth_providers.toml`) so they survive MCP server add/remove.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DaemonMcpConfig {
    #[serde(default)]
    pub servers: HashMap<String, DaemonServerConfig>,
}

impl DaemonMcpConfig {
    /// Extract the core `McpConfig` for `McpRegistry` initialization.
    pub fn to_core_config(&self) -> simply_core::McpConfig {
        simply_core::McpConfig {
            servers: self.servers.iter()
                .map(|(id, cfg)| (id.clone(), cfg.core()))
                .collect(),
        }
    }

    /// Build the static bearer tokens map for `start_auto_connect`.
    /// Only includes Token-auth servers. OAuth servers connect per-user on demand.
    pub fn static_bearer_tokens(&self) -> HashMap<String, String> {
        self.servers.iter()
            .filter_map(|(id, cfg)| {
                cfg.static_bearer_token().map(|t| (id.clone(), t.to_string()))
            })
            .collect()
    }

    /// IDs of servers that should be auto-connected globally (non-OAuth servers).
    pub fn global_auto_connect_ids(&self) -> Vec<String> {
        self.servers.iter()
            .filter(|(_, cfg)| cfg.auto_connect && cfg.should_global_connect())
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn get_server(&self, id: &str) -> Option<&DaemonServerConfig> {
        self.servers.get(id)
    }

    pub fn get_server_mut(&mut self, id: &str) -> Option<&mut DaemonServerConfig> {
        self.servers.get_mut(id)
    }

    pub fn add_server(&mut self, id: String, config: DaemonServerConfig) {
        self.servers.insert(id, config);
    }

    pub fn remove_server(&mut self, id: &str) -> Option<DaemonServerConfig> {
        self.servers.remove(id)
    }
}
