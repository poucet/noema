//! ToolRegistry — unified dispatch across all tool providers.
//!
//! Replaces the old CompositeToolService. Just a list of ToolProviders
//! (MCP servers, WS skills, embedded skills) + daemon REST tools.
//! Also implements McpApi by delegating management to McpService.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use simply_rpc::{RequestContext, RestService};
use tokio::sync::RwLock;

use simply_core::ToolService;
use simply_daemon_api::ToolProvider;

use crate::api::*;
use crate::mcp::McpService;
use super::tools::DaemonToolService;

/// Unified tool registry — all tool providers in one place.
pub struct ToolRegistry {
    /// Daemon's own REST API methods exposed as tools.
    daemon_tools: Arc<DaemonToolService>,
    /// All registered tool providers (MCP servers, WS skills, embedded skills).
    providers: RwLock<Vec<Arc<dyn ToolProvider>>>,
    /// Token store for populating RequestContext.tokens.
    token_store: Arc<super::token_store::TransientTokenStore>,
    /// MCP service for server management + OAuth.
    mcp: Arc<McpService>,
}

impl ToolRegistry {
    pub fn new(
        daemon_tools: Arc<DaemonToolService>,
        token_store: Arc<super::token_store::TransientTokenStore>,
        mcp: Arc<McpService>,
    ) -> Self {
        Self {
            daemon_tools,
            providers: RwLock::new(Vec::new()),
            token_store,
            mcp,
        }
    }

    /// Register a tool provider.
    pub async fn register(&self, provider: Arc<dyn ToolProvider>) {
        let tools = provider.tools().await;
        tracing::info!(
            id = provider.id(),
            name = provider.display_name(),
            tools = tools.len(),
            "registered tool provider"
        );
        self.providers.write().await.push(provider);
    }

    /// Unregister a provider by ID.
    pub async fn unregister(&self, id: &str) {
        self.providers.write().await.retain(|p| p.id() != id);
        tracing::info!(id, "unregistered tool provider");
    }

    /// Get all registered providers.
    pub async fn providers(&self) -> Vec<Arc<dyn ToolProvider>> {
        self.providers.read().await.clone()
    }

    /// Get the daemon REST tool services (for ServiceRouter registration).
    pub fn daemon_tool_services(&self) -> &[Arc<dyn RestService>] {
        self.daemon_tools.services()
    }

    /// Access the underlying MCP service (for OAuth tracker, etc.).
    pub fn mcp_service(&self) -> &Arc<McpService> {
        &self.mcp
    }

    /// Build a RequestContext with tokens for a user.
    pub async fn ctx_with_tokens(&self, user_id: &simply_core::storage::ids::UserId) -> RequestContext {
        let mut ctx = RequestContext::with_scope(
            simply_rpc::Scope::user(user_id.as_str()),
        );

        let registry = self.mcp.registry().lock().await;

        // Populate tokens from TransientTokenStore (per-user OAuth)
        for (server_id, _) in registry.config().servers.iter() {
            if let Some(token) = self.token_store.get(user_id, server_id) {
                ctx.tokens.insert(server_id.clone(), token.access_token);
            }
        }

        ctx
    }

    /// Get a user-scoped ToolService for a session (daemon tools + all providers).
    pub async fn for_user(
        &self,
        user_id: &simply_core::storage::ids::UserId,
    ) -> Arc<dyn ToolService> {
        let ctx = self.ctx_with_tokens(user_id).await;
        let scoped_daemon = Arc::new(self.daemon_tools.with_context(ctx.clone()));
        let providers = self.providers.read().await.clone();
        Arc::new(UserScopedTools { daemon_tools: scoped_daemon, providers, ctx })
    }
}

// ---------------------------------------------------------------------------
// ToolService — dispatches across daemon REST tools + providers
// ---------------------------------------------------------------------------

#[async_trait]
impl ToolService for ToolRegistry {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = self.daemon_tools.get_definitions().await;
        for provider in self.providers.read().await.iter() {
            for tool in provider.tools().await {
                defs.push(mcp_tool_to_definition(&tool));
            }
        }
        defs
    }

    async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<Vec<ToolResultContent>> {
        // Daemon REST tools
        if self.daemon_tools.get_definitions().await.iter().any(|d| d.name == name) {
            return self.daemon_tools.call_tool(name, arguments).await;
        }
        // Providers (anonymous context)
        let ctx = RequestContext::anonymous();
        let request = CallToolRequestParams::new(name.to_string())
            .with_arguments(arguments.as_object().cloned().unwrap_or_default());
        for provider in self.providers.read().await.iter() {
            if provider.tools().await.iter().any(|t| t.name.as_ref() == name) {
                let result = provider.call_tool(request, &ctx).await?;
                return Ok(call_result_to_content(result));
            }
        }
        anyhow::bail!("tool not found: {name}")
    }
}

// ---------------------------------------------------------------------------
// McpApi — delegates server management to McpService, tool ops to self
// ---------------------------------------------------------------------------

#[async_trait]
impl McpApi for ToolRegistry {
    async fn list_mcp_servers(&self) -> anyhow::Result<Vec<McpServerInfo>> { self.mcp.list_mcp_servers().await }
    async fn add_mcp_server(&self, request: AddMcpServerRequest) -> anyhow::Result<()> { self.mcp.add_mcp_server(request).await }
    async fn remove_mcp_server(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.remove_mcp_server(server_id).await }
    async fn connect_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> { self.mcp.connect_mcp_server(server_id).await }
    async fn disconnect_mcp_server(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.disconnect_mcp_server(server_id).await }
    async fn get_mcp_server_tools(&self, server_id: &str) -> anyhow::Result<Vec<McpTool>> { self.mcp.get_mcp_server_tools(server_id).await }
    async fn update_mcp_server_settings(&self, server_id: &str, request: UpdateMcpServerRequest) -> anyhow::Result<()> { self.mcp.update_mcp_server_settings(server_id, request).await }
    async fn stop_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.stop_mcp_retry(server_id).await }
    async fn start_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.start_mcp_retry(server_id).await }
    async fn register_ephemeral_mcp(&self, request: RegisterEphemeralRequest) -> anyhow::Result<usize> { self.mcp.register_ephemeral_mcp(request).await }
    async fn unregister_ephemeral_mcp(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.unregister_ephemeral_mcp(server_id).await }

    async fn list_all_tools(&self, _ctx: &RequestContext) -> anyhow::Result<Vec<McpTool>> {
        let defs = self.get_definitions().await;
        Ok(defs.into_iter().map(|d| {
            let schema = serde_json::to_value(&d.input_schema).unwrap_or_default();
            let schema_map = schema.as_object().cloned().unwrap_or_default();
            McpTool::new(d.name, d.description.unwrap_or_default(), schema_map)
        }).collect())
    }

    async fn call_tool_direct(&self, ctx: &RequestContext, request: CallToolRequestParams) -> anyhow::Result<CallToolResult> {
        let name = request.name.as_ref();
        let args = request.arguments
            .map(serde_json::Value::Object)
            .unwrap_or_default();

        tracing::info!(
            tool = name,
            user_id = ?ctx.scope.user_id,
            "call_tool_direct: dispatching"
        );

        // Get user-scoped or global tool service
        let content = if let Some(ref user_id) = ctx.scope.user_id {
            let uid = simply_core::storage::ids::UserId::from_string(user_id);
            let scoped = self.for_user(&uid).await;
            scoped.call_tool(name, args).await?
        } else {
            self.call_tool(name, args).await?
        };

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

// ---------------------------------------------------------------------------
// UserScopedTools — per-session tool service
// ---------------------------------------------------------------------------

struct UserScopedTools {
    daemon_tools: Arc<DaemonToolService>,
    providers: Vec<Arc<dyn ToolProvider>>,
    ctx: RequestContext,
}

#[async_trait]
impl ToolService for UserScopedTools {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = self.daemon_tools.get_definitions().await;
        for provider in &self.providers {
            for tool in provider.tools().await {
                defs.push(mcp_tool_to_definition(&tool));
            }
        }
        defs
    }

    async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<Vec<ToolResultContent>> {
        // Daemon REST tools (user-scoped)
        if self.daemon_tools.get_definitions().await.iter().any(|d| d.name == name) {
            return self.daemon_tools.call_tool(name, arguments).await;
        }
        // Providers (with user tokens)
        let request = CallToolRequestParams::new(name.to_string())
            .with_arguments(arguments.as_object().cloned().unwrap_or_default());
        for provider in &self.providers {
            if provider.tools().await.iter().any(|t| t.name.as_ref() == name) {
                let result = provider.call_tool(request, &self.ctx).await?;
                return Ok(call_result_to_content(result));
            }
        }
        anyhow::bail!("tool not found: {name}")
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn mcp_tool_to_definition(tool: &Tool) -> ToolDefinition {
    let schema = serde_json::to_value(&*tool.input_schema).unwrap_or_default();
    ToolDefinition {
        name: tool.name.to_string(),
        description: tool.description.as_ref().map(|d| d.to_string()),
        input_schema: serde_json::from_value(schema).unwrap_or_default(),
    }
}

fn call_result_to_content(result: CallToolResult) -> Vec<ToolResultContent> {
    result.content.into_iter().map(|c| {
        match c.raw {
            rmcp::model::RawContent::Text(t) => ToolResultContent::text(t.text),
            rmcp::model::RawContent::Image(img) => ToolResultContent::image(img.data, img.mime_type),
            _ => ToolResultContent::text("[unsupported content type]"),
        }
    }).collect()
}
