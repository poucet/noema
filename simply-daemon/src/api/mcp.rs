//! MCP service registration and tool discovery.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// MCP service registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRegistration {
    pub name: String,
    pub endpoint: String,
}

#[async_trait]
pub trait McpApi: Send + Sync {
    /// Register an MCP service. Tools become globally available.
    async fn register_mcp(&self, registration: McpRegistration) -> anyhow::Result<()>;

    /// Unregister an MCP service.
    async fn unregister_mcp(&self, name: &str) -> anyhow::Result<()>;

    /// List all registered MCP tool servers.
    async fn list_tools(&self) -> anyhow::Result<Vec<String>>;
}
