use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Minimal connection config for an MCP server.
///
/// Auth is intentionally absent — simply-core is auth-agnostic.
/// The daemon resolves auth externally and passes a bearer token
/// to `McpRegistry::connect` / `connect_to_server`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Display name for the server.
    pub name: String,
    /// HTTP endpoint URL for the streamable HTTP server.
    pub url: String,
    /// Automatically connect to this server on app startup.
    #[serde(default = "default_true")]
    pub auto_connect: bool,
    /// Enable automatic retry with exponential backoff when connection fails.
    #[serde(default = "default_true")]
    pub auto_retry: bool,
}

fn default_true() -> bool {
    true
}

/// In-memory collection of MCP server connection configs.
///
/// No auth, no credentials — those are daemon-side concerns.
/// No file I/O — loading and saving is the daemon's responsibility.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: HashMap<String, ServerConfig>,
}

impl McpConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_server(&mut self, id: String, config: ServerConfig) {
        self.servers.insert(id, config);
    }

    pub fn remove_server(&mut self, id: &str) -> Option<ServerConfig> {
        self.servers.remove(id)
    }

    pub fn get_server(&self, id: &str) -> Option<&ServerConfig> {
        self.servers.get(id)
    }
}
