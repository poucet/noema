use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Display name for the server
    pub name: String,
    /// HTTP endpoint URL for the streamable HTTP server
    pub url: String,
    /// Optional authentication token
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
