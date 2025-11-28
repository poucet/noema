use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Authentication method for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthMethod {
    /// No authentication required
    #[default]
    None,
    /// Static bearer token
    Token {
        token: String,
    },
    /// OAuth 2.0 authentication
    OAuth {
        /// OAuth client ID
        client_id: String,
        /// OAuth client secret (optional for public clients)
        #[serde(skip_serializing_if = "Option::is_none")]
        client_secret: Option<String>,
        /// Authorization endpoint URL (discovered via .well-known if not set)
        #[serde(skip_serializing_if = "Option::is_none")]
        authorization_url: Option<String>,
        /// Token endpoint URL (discovered via .well-known if not set)
        #[serde(skip_serializing_if = "Option::is_none")]
        token_url: Option<String>,
        /// Requested scopes
        #[serde(default)]
        scopes: Vec<String>,
        /// Current access token (populated after OAuth flow)
        #[serde(skip_serializing_if = "Option::is_none")]
        access_token: Option<String>,
        /// Refresh token for obtaining new access tokens
        #[serde(skip_serializing_if = "Option::is_none")]
        refresh_token: Option<String>,
        /// Token expiration timestamp (Unix epoch seconds)
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<i64>,
    },
}

impl AuthMethod {
    /// Get the current bearer token for authorization header
    pub fn bearer_token(&self) -> Option<&str> {
        match self {
            AuthMethod::None => None,
            AuthMethod::Token { token } => Some(token),
            AuthMethod::OAuth { access_token, .. } => access_token.as_deref(),
        }
    }

    /// Check if OAuth token is expired or about to expire (within 60 seconds)
    pub fn is_token_expired(&self) -> bool {
        match self {
            AuthMethod::OAuth { expires_at: Some(expires), .. } => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                *expires <= now + 60
            }
            _ => false,
        }
    }

    /// Check if this auth method requires OAuth login
    pub fn needs_oauth_login(&self) -> bool {
        matches!(self, AuthMethod::OAuth { access_token: None, .. })
    }
}

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Display name for the server
    pub name: String,
    /// HTTP endpoint URL for the streamable HTTP server
    pub url: String,
    /// Authentication method
    #[serde(default)]
    pub auth: AuthMethod,
    /// Whether to use .well-known discovery for OAuth endpoints
    #[serde(default)]
    pub use_well_known: bool,
    /// Optional authentication token (legacy, prefer `auth` field)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
}

/// Root configuration containing all MCP servers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: HashMap<String, ServerConfig>,
}

impl McpConfig {
    /// Get the default config file path (~/.noema/mcp.toml)
    pub fn default_path() -> Option<PathBuf> {
        directories::UserDirs::new().map(|dirs| dirs.home_dir().join(".noema").join("mcp.toml"))
    }

    /// Load configuration from the default path
    pub fn load() -> anyhow::Result<Self> {
        match Self::default_path() {
            Some(path) if path.exists() => Self::load_from(&path),
            Some(_) => Ok(Self::default()),
            None => Ok(Self::default()),
        }
    }

    /// Load configuration from a specific path
    pub fn load_from(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: McpConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to the default path
    pub fn save(&self) -> anyhow::Result<()> {
        match Self::default_path() {
            Some(path) => self.save_to(&path),
            None => Err(anyhow::anyhow!("Could not determine config path")),
        }
    }

    /// Save configuration to a specific path
    pub fn save_to(&self, path: &PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Add a new server configuration
    pub fn add_server(&mut self, id: String, config: ServerConfig) {
        self.servers.insert(id, config);
    }

    /// Remove a server configuration
    pub fn remove_server(&mut self, id: &str) -> Option<ServerConfig> {
        self.servers.remove(id)
    }

    /// Get a server configuration by ID
    pub fn get_server(&self, id: &str) -> Option<&ServerConfig> {
        self.servers.get(id)
    }
}
