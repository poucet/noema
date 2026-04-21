//! MCP service registration, discovery, and configuration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use simply_rpc::RequestContext;
#[cfg(feature = "ts")]
use ts_rs::TS;

// Re-export rmcp types used in the API.
pub use rmcp::model::{
    CallToolRequestParams, CallToolResult, Tool as McpTool,
};

/// Information about a configured MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
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
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct AddMcpServerRequest {
    pub id: String,
    pub name: String,
    pub url: String,
    /// "none", "token", or "oauth".
    pub auth_type: String,
    /// Static bearer token for `auth_type = "token"`.
    pub auth_token: Option<String>,
    /// OAuth provider ID (must exist in oauth_providers.toml) for `auth_type = "oauth"`.
    pub provider_id: Option<String>,
    /// Scopes required by this server.
    pub scopes: Option<Vec<String>>,
}

/// Request to update MCP server settings.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
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

    // -- Tool operations (user-scoped) --

    /// List all tools across all connected servers (includes schemas).
    #[rpc(get = "/mcp/tools", no_tool)]
    async fn list_all_tools(&self, ctx: &RequestContext) -> anyhow::Result<Vec<McpTool>>;

    /// Call a tool by name (routed via ToolService to the providing server).
    #[rpc(post = "/mcp/tools/call", no_tool)]
    async fn call_tool_direct(&self, ctx: &RequestContext, request: CallToolRequestParams) -> anyhow::Result<CallToolResult>;
}

/// Implement ToolService for the remote MCP client so it can be used as a tool provider.
#[async_trait]
impl simply_core::ToolService for RemoteMcpApi {
    async fn get_definitions(&self) -> Vec<llm::ToolDefinition> {
        let anon = simply_rpc::RequestContext::anonymous();
        McpApi::list_all_tools(self, &anon).await
            .unwrap_or_default()
            .into_iter()
            .map(|t| llm::ToolDefinition {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()),
                input_schema: serde_json::from_value(
                    serde_json::to_value(&*t.input_schema).unwrap_or_default()
                ).unwrap_or_default(),
            })
            .collect()
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<Vec<llm::ToolResultContent>> {
        let anon = simply_rpc::RequestContext::anonymous();
        let result = McpApi::call_tool_direct(
            self,
            &anon,
            CallToolRequestParams::new(name.to_string())
                .with_arguments(arguments.as_object().cloned().unwrap_or_default()),
        ).await?;

        let content: Vec<llm::ToolResultContent> = result.content.into_iter().map(|c| {
            match c.raw {
                rmcp::model::RawContent::Text(t) => llm::ToolResultContent::text(t.text),
                rmcp::model::RawContent::Image(img) => llm::ToolResultContent::image(img.data, img.mime_type),
                _ => llm::ToolResultContent::text("[unsupported content type]"),
            }
        }).collect();

        Ok(content)
    }
}
