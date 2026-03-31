//! MCP service registration, discovery, and configuration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Information about a configured MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_type: String,
    pub is_connected: bool,
    pub tool_count: usize,
    pub status: String,
}

/// Information about an MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
}

/// Request to add a new MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMcpServerRequest {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_type: String,
    pub auth_token: Option<String>,
}

/// Request to update MCP server settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMcpServerRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    pub auto_connect: Option<bool>,
    pub auto_retry: Option<bool>,
}

#[async_trait]
pub trait McpApi: Send + Sync {
    /// List all configured MCP servers with their status.
    async fn list_mcp_servers(&self) -> anyhow::Result<Vec<McpServerInfo>>;

    /// Add a new MCP server configuration.
    async fn add_mcp_server(&self, request: AddMcpServerRequest) -> anyhow::Result<()>;

    /// Remove an MCP server configuration.
    async fn remove_mcp_server(&self, server_id: &str) -> anyhow::Result<()>;

    /// Connect to an MCP server. Returns tool count.
    async fn connect_mcp_server(&self, server_id: &str) -> anyhow::Result<usize>;

    /// Disconnect from an MCP server.
    async fn disconnect_mcp_server(&self, server_id: &str) -> anyhow::Result<()>;

    /// Get tools provided by a specific MCP server.
    async fn get_mcp_server_tools(&self, server_id: &str) -> anyhow::Result<Vec<McpToolInfo>>;

    /// Test connection to an MCP server. Returns tool count.
    async fn test_mcp_server(&self, server_id: &str) -> anyhow::Result<usize>;

    /// Update settings for an MCP server.
    async fn update_mcp_server_settings(
        &self,
        server_id: &str,
        request: UpdateMcpServerRequest,
    ) -> anyhow::Result<()>;

    /// Stop retry attempts for an MCP server.
    async fn stop_mcp_retry(&self, server_id: &str) -> anyhow::Result<()>;

    /// Start retry attempts for an MCP server.
    async fn start_mcp_retry(&self, server_id: &str) -> anyhow::Result<()>;
}
