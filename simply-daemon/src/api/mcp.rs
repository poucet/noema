//! MCP service registration, discovery, and configuration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use simply_rpc::RequestContext;

// Re-export rmcp types used in the API.
pub use rmcp::model::{
    CallToolRequestParams, CallToolResult, Tool as McpTool,
};

/// Information about a configured MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_type: String,
    pub is_connected: bool,
    pub needs_oauth_login: bool,
    pub tool_count: usize,
    pub status: String,
    pub auto_connect: bool,
    pub auto_retry: bool,
}

/// Request to add a new MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddMcpServerRequest {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_type: String,
    pub auth_token: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub scopes: Option<Vec<String>>,
}

/// Request to register an ephemeral MCP server at runtime.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RegisterEphemeralRequest {
    pub id: String,
    pub url: String,
}

/// Request to update MCP server settings.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateMcpServerRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    pub auto_connect: Option<bool>,
    pub auto_retry: Option<bool>,
}

#[simply_rpc::rpc_service("mcp")]
#[async_trait]
pub trait McpApi: Send + Sync {
    // -- Server management (no user context needed) --

    /// List all configured MCP servers with their status.
    #[rpc(get = "/mcp")]
    async fn list_mcp_servers(&self) -> anyhow::Result<Vec<McpServerInfo>>;

    /// Add a new MCP server configuration.
    #[rpc(post = "/mcp")]
    async fn add_mcp_server(&self, request: AddMcpServerRequest) -> anyhow::Result<()>;

    /// Remove an MCP server configuration.
    #[rpc(delete = "/mcp/{server_id}")]
    async fn remove_mcp_server(&self, server_id: &str) -> anyhow::Result<()>;

    /// Connect to an MCP server. Returns tool count.
    #[rpc(post = "/mcp/{server_id}/connect")]
    async fn connect_mcp_server(&self, server_id: &str) -> anyhow::Result<usize>;

    /// Disconnect from an MCP server.
    #[rpc(post = "/mcp/{server_id}/disconnect")]
    async fn disconnect_mcp_server(&self, server_id: &str) -> anyhow::Result<()>;

    /// Get tools provided by a specific MCP server.
    #[rpc(get = "/mcp/{server_id}/tools")]
    async fn get_mcp_server_tools(&self, server_id: &str) -> anyhow::Result<Vec<McpTool>>;

    /// Update settings for an MCP server.
    #[rpc(put = "/mcp/{server_id}")]
    async fn update_mcp_server_settings(&self, server_id: &str, request: UpdateMcpServerRequest) -> anyhow::Result<()>;

    /// Stop retry attempts for an MCP server.
    #[rpc(post = "/mcp/{server_id}/stop-retry")]
    async fn stop_mcp_retry(&self, server_id: &str) -> anyhow::Result<()>;

    /// Start retry attempts for an MCP server.
    #[rpc(post = "/mcp/{server_id}/retry")]
    async fn start_mcp_retry(&self, server_id: &str) -> anyhow::Result<()>;

    /// Register an ephemeral MCP server and connect to it.
    #[rpc(post = "/mcp/ephemeral", no_tool)]
    async fn register_ephemeral_mcp(&self, request: RegisterEphemeralRequest) -> anyhow::Result<usize>;

    /// Unregister an ephemeral MCP server and disconnect.
    #[rpc(delete = "/mcp/ephemeral/{server_id}", no_tool)]
    async fn unregister_ephemeral_mcp(&self, server_id: &str) -> anyhow::Result<()>;

    // -- Tool operations (user-scoped) --

    /// List all tools across all connected servers (includes schemas).
    #[rpc(get = "/mcp/tools", no_tool)]
    async fn list_all_tools(&self, ctx: &RequestContext) -> anyhow::Result<Vec<McpTool>>;

    /// Call a tool by name (routed via ToolService to the providing server).
    #[rpc(post = "/mcp/tools/call", no_tool)]
    async fn call_tool_direct(&self, ctx: &RequestContext, request: CallToolRequestParams) -> anyhow::Result<CallToolResult>;
}
