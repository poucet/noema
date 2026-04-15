//! Daemon tool services — expose daemon REST APIs as tools for agents.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};
use simply_rpc::{BinaryResponse, RestService};

use simply_core::{McpToolRegistry, ToolService};
use crate::api::*;
use crate::mcp::McpService;

/// Convert an RPC result `Value` to `ToolResultContent`, using route metadata
/// to determine if the result is binary (image/audio) or JSON text.
fn value_to_tool_result(value: serde_json::Value, binary_response: bool) -> Vec<ToolResultContent> {
    if binary_response {
        use base64::Engine;
        return match serde_json::from_value::<BinaryResponse>(value) {
            Ok(binary) => {
                let data = base64::engine::general_purpose::STANDARD.encode(&binary.data);
                if binary.mime_type.starts_with("image/") {
                    vec![ToolResultContent::image(data, binary.mime_type)]
                } else if binary.mime_type.starts_with("audio/") {
                    vec![ToolResultContent::audio(data, binary.mime_type)]
                } else {
                    vec![ToolResultContent::text(format!("[binary: {} bytes, {}]", binary.data.len(), binary.mime_type))]
                }
            }
            Err(e) => vec![ToolResultContent::text(format!("failed to parse binary response: {e}"))],
        };
    }
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    vec![ToolResultContent::text(text)]
}

/// Exposes daemon REST methods as tools.
///
/// Holds a `RequestContext` that's passed to every dispatch call.
/// Create a scoped instance via `with_context()` for per-user sessions.
pub struct DaemonToolService {
    services: Vec<Arc<dyn RestService>>,
    ctx: simply_rpc::RequestContext,
}

impl DaemonToolService {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            ctx: simply_rpc::RequestContext::default(),
        }
    }

    pub fn register(mut self, svc: Arc<dyn RestService>) -> Self {
        self.services.push(svc);
        self
    }

    /// Create a new DaemonToolService with the same services but a different context.
    /// Used to scope daemon tools to a specific user's session.
    pub fn with_context(&self, ctx: simply_rpc::RequestContext) -> Self {
        Self {
            services: self.services.clone(),
            ctx,
        }
    }
}

#[async_trait]
impl ToolService for DaemonToolService {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.services
            .iter()
            .flat_map(|svc| {
                svc.meta().routes.iter().filter_map(|rm| {
                    if rm.no_tool || !rm.is_rest() { return None; }
                    Some(ToolDefinition {
                        name: rm.method_name.to_string(),
                        description: rm.description.map(|s| s.to_string()),
                        input_schema: (rm.tool_schema)().cloned().unwrap_or_default(),
                    })
                })
            })
            .collect()
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<Vec<ToolResultContent>> {
        for svc in &self.services {
            let route = svc.meta().routes.iter().find(|rm| rm.method_name == name);
            if let Some(rm) = route {
                if let Some(result) = svc.rest_dispatch_by_name(name, self.ctx.clone(), arguments.clone()).await {
                    let value = result?;
                    return Ok(value_to_tool_result(value, rm.binary_response));
                }
            }
        }
        anyhow::bail!("tool not found: {name}")
    }
}

impl Default for DaemonToolService {
    fn default() -> Self { Self::new() }
}

// ---------------------------------------------------------------------------
// CompositeToolService — daemon REST tools + MCP tools, implements McpApi
// ---------------------------------------------------------------------------

/// Combines daemon REST tools and MCP tools into a single service.
/// Implements both `ToolService` (for in-process use) and `McpApi` (for REST).
pub struct CompositeToolService {
    daemon_tools: DaemonToolService,
    mcp_tools: McpToolRegistry,
    mcp: Arc<McpService>,
}

impl CompositeToolService {
    pub fn new(
        daemon_tools: DaemonToolService,
        mcp_tools: McpToolRegistry,
        mcp: Arc<McpService>,
    ) -> Self {
        Self { daemon_tools, mcp_tools, mcp }
    }
}

#[async_trait]
impl ToolService for CompositeToolService {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = self.daemon_tools.get_definitions().await;
        defs.extend(self.mcp_tools.get_definitions().await);
        defs
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<Vec<ToolResultContent>> {
        // Try daemon tools first, then MCP
        if self.daemon_tools.get_definitions().await.iter().any(|d| d.name == name) {
            return self.daemon_tools.call_tool(name, arguments).await;
        }
        self.mcp_tools.call_tool(name, arguments).await
    }
}

#[async_trait]
impl McpApi for CompositeToolService {
    // MCP management — delegate to inner McpService
    async fn list_mcp_servers(&self) -> anyhow::Result<Vec<McpServerInfo>> { self.mcp.list_mcp_servers().await }
    async fn add_mcp_server(&self, request: AddMcpServerRequest) -> anyhow::Result<()> { self.mcp.add_mcp_server(request).await }
    async fn remove_mcp_server(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.remove_mcp_server(server_id).await }
    async fn connect_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> { self.mcp.connect_mcp_server(server_id).await }
    async fn disconnect_mcp_server(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.disconnect_mcp_server(server_id).await }
    async fn get_mcp_server_tools(&self, server_id: &str) -> anyhow::Result<Vec<McpTool>> { self.mcp.get_mcp_server_tools(server_id).await }
    async fn test_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> { self.mcp.test_mcp_server(server_id).await }
    async fn update_mcp_server_settings(&self, server_id: &str, request: UpdateMcpServerRequest) -> anyhow::Result<()> { self.mcp.update_mcp_server_settings(server_id, request).await }
    async fn stop_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.stop_mcp_retry(server_id).await }
    async fn start_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.start_mcp_retry(server_id).await }
    async fn register_ephemeral_mcp(&self, request: RegisterEphemeralRequest) -> anyhow::Result<usize> { self.mcp.register_ephemeral_mcp(request).await }
    async fn unregister_ephemeral_mcp(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.unregister_ephemeral_mcp(server_id).await }

    // Tool listing/calling — use the composite
    async fn list_all_tools(&self) -> anyhow::Result<Vec<McpTool>> {
        let defs = self.get_definitions().await;
        Ok(defs.into_iter().map(|d| {
            let schema = serde_json::to_value(&d.input_schema).unwrap_or_default();
            let schema_map = schema.as_object().cloned().unwrap_or_default();
            McpTool::new(d.name, d.description.unwrap_or_default(), schema_map)
        }).collect())
    }

    async fn call_tool_direct(&self, request: CallToolRequestParam) -> anyhow::Result<CallToolResult> {
        let name = request.name.as_ref();
        let args = request.arguments
            .map(serde_json::Value::Object)
            .unwrap_or(serde_json::Value::Object(Default::default()));

        let content = self.call_tool(name, args).await?;

        let mcp_content: Vec<rmcp::model::Content> = content.into_iter().map(|c| {
            match c {
                ToolResultContent::Text { text } => rmcp::model::Content::text(text),
                ToolResultContent::Image { data, mime_type } => rmcp::model::Content::image(data, mime_type),
                _ => rmcp::model::Content::text("[unsupported content type]"),
            }
        }).collect();

        Ok(CallToolResult::success(mcp_content))
    }
}
