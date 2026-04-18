//! Daemon tool services — expose daemon REST APIs as tools for agents.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};
use rmcp::model::CallToolRequestParams;
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

    /// Access the registered service list (for ServiceRouter, etc.)
    pub fn services(&self) -> &[Arc<dyn RestService>] {
        &self.services
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
                if let Some(result) = svc.rest_dispatch_by_name(name, &self.ctx, arguments.clone()).await {
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

/// Combines daemon REST tools, MCP tools, and skills into a single service.
/// Implements both `ToolService` (for in-process use) and `McpApi` (for REST).
pub struct CompositeToolService {
    daemon_tools: Arc<DaemonToolService>,
    mcp_tools: McpToolRegistry,
    mcp: Arc<McpService>,
    user_tools: Arc<crate::services::user_tools::UserToolServiceCache>,
    token_store: Arc<crate::services::token_store::TransientTokenStore>,
    skills: tokio::sync::RwLock<Vec<Arc<dyn simply_daemon_api::Skill>>>,
}

impl CompositeToolService {
    pub fn new(
        daemon_tools: Arc<DaemonToolService>,
        mcp_tools: McpToolRegistry,
        mcp: Arc<McpService>,
        user_tools: Arc<crate::services::user_tools::UserToolServiceCache>,
        token_store: Arc<crate::services::token_store::TransientTokenStore>,
    ) -> Self {
        Self { daemon_tools, mcp_tools, mcp, user_tools, token_store, skills: tokio::sync::RwLock::new(Vec::new()) }
    }

    /// Register a skill at runtime (after daemon construction).
    pub async fn register_skill_runtime(&self, skill: Box<dyn simply_daemon_api::Skill>) {
        let skill: Arc<dyn simply_daemon_api::Skill> = skill.into();
        tracing::info!(skill = skill.name(), tools = skill.tools().len(), "registered skill (runtime)");
        self.skills.write().await.push(skill);
    }

    /// Build a SkillCallContext for a user, populating tokens from the store.
    async fn skill_context_for(&self, user_id: &simply_core::storage::ids::UserId) -> simply_daemon_api::SkillCallContext {
        let mut ctx = simply_daemon_api::SkillCallContext::new(user_id.clone());
        // Populate tokens from all known MCP servers
        let registry = self.mcp.registry().lock().await;
        for (server_id, _) in registry.config().servers.iter() {
            if let Some(token) = self.token_store.get(user_id, server_id) {
                ctx = ctx.with_token(server_id.clone(), token.access_token);
            }
        }
        // Also check global server tokens (from OAuthService flow)
        for (server_id, config) in registry.config().servers.iter() {
            if let simply_core::AuthMethod::OAuth { access_token: Some(ref token), .. } = config.auth {
                if !ctx.tokens.contains_key(server_id) {
                    ctx = ctx.with_token(server_id.clone(), token.clone());
                }
            }
        }
        ctx
    }

    /// Get the daemon tool services (same list used for REST dispatch).
    pub fn daemon_tool_services(&self) -> Vec<Arc<dyn RestService>> {
        self.daemon_tools.services().to_vec()
    }

    /// Get a user-scoped tool service for a session.
    pub async fn for_user(
        self: &Arc<Self>,
        user_id: &simply_core::storage::ids::UserId,
    ) -> anyhow::Result<Arc<dyn ToolService>> {
        self.for_user_internal(user_id).await.map(|s| s as Arc<dyn ToolService>)
    }

    async fn for_user_internal(
        &self,
        user_id: &simply_core::storage::ids::UserId,
    ) -> anyhow::Result<Arc<UserScopedToolService>> {
        let scoped_daemon = Arc::new(self.daemon_tools.with_context(
            simply_rpc::RequestContext::with_scope(
                simply_rpc::Scope::user(user_id.as_str()),
            ),
        ));
        let mcp_callers = self.user_tools.build_mcp_callers_for(user_id).await?;
        let skill_ctx = self.skill_context_for(user_id).await;
        let skills = self.skills.read().await.clone();

        Ok(Arc::new(UserScopedToolService {
            daemon_tools: scoped_daemon,
            mcp_callers,
            skills,
            skill_ctx,
            mcp_fallback: Arc::clone(&self.mcp),
        }))
    }
}

/// A user-scoped tool service — daemon tools scoped with user context,
/// per-user MCP connections, skills with user tokens.
struct UserScopedToolService {
    daemon_tools: Arc<DaemonToolService>,
    mcp_callers: Vec<crate::services::user_tools::McpCaller>,
    skills: Vec<Arc<dyn simply_daemon_api::Skill>>,
    skill_ctx: simply_daemon_api::SkillCallContext,
    mcp_fallback: Arc<McpService>,
}

#[async_trait]
impl ToolService for UserScopedToolService {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = self.daemon_tools.get_definitions().await;
        for caller in &self.mcp_callers {
            defs.extend(caller.tools.iter().cloned());
        }
        for skill in &self.skills {
            defs.extend(skill.tools());
        }
        defs
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<Vec<ToolResultContent>> {
        // 1. Try daemon REST tools (user-scoped)
        if self.daemon_tools.get_definitions().await.iter().any(|d| d.name == name) {
            return self.daemon_tools.call_tool(name, arguments).await;
        }

        // 2. Try skills (with user's token context)
        for skill in &self.skills {
            if skill.tools().iter().any(|t| t.name == name) {
                return skill.call_tool(name, arguments, &self.skill_ctx).await;
            }
        }

        // 3. Try per-user MCP callers
        let args_map = match &arguments {
            serde_json::Value::Object(map) => Some(map.clone()),
            _ => None,
        };
        for caller in &self.mcp_callers {
            if caller.tools.iter().any(|t| t.name == name) {
                let result = caller.call_tool(name.to_string(), args_map).await?;
                let content: Vec<ToolResultContent> = result.content.into_iter().map(|c| {
                    match c.raw {
                        rmcp::model::RawContent::Text(t) => ToolResultContent::text(t.text),
                        rmcp::model::RawContent::Image(img) => ToolResultContent::image(img.data, img.mime_type),
                        _ => ToolResultContent::text("[unsupported content type]"),
                    }
                }).collect();
                return Ok(content);
            }
        }

        // 4. Fall back to global MCP registry
        let anon = simply_rpc::RequestContext::anonymous();
        let request = rmcp::model::CallToolRequestParams::new(name.to_string())
            .with_arguments(arguments.as_object().cloned().unwrap_or_default());
        match self.mcp_fallback.call_tool_direct(&anon, request).await {
            Ok(result) => {
                Ok(result.content.into_iter().map(|c| match c.raw {
                    rmcp::model::RawContent::Text(t) => ToolResultContent::text(t.text),
                    rmcp::model::RawContent::Image(img) => ToolResultContent::image(img.data, img.mime_type),
                    _ => ToolResultContent::text("[unsupported content type]"),
                }).collect())
            }
            Err(_) => anyhow::bail!("tool not found: {name}"),
        }
    }
}

#[async_trait]
impl ToolService for CompositeToolService {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = self.daemon_tools.get_definitions().await;
        defs.extend(self.mcp_tools.get_definitions().await);
        for skill in self.skills.read().await.iter() {
            defs.extend(skill.tools());
        }
        defs
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<Vec<ToolResultContent>> {
        // Try daemon tools first
        if self.daemon_tools.get_definitions().await.iter().any(|d| d.name == name) {
            return self.daemon_tools.call_tool(name, arguments).await;
        }
        // Try skills (anonymous context — call_tool_direct handles user context)
        for skill in self.skills.read().await.iter() {
            if skill.tools().iter().any(|t| t.name == name) {
                let ctx = simply_daemon_api::SkillCallContext::new(
                    simply_core::storage::ids::UserId::from_string("anonymous"),
                );
                return skill.call_tool(name, arguments, &ctx).await;
            }
        }
        // Fall back to MCP
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
    async fn update_mcp_server_settings(&self, server_id: &str, request: UpdateMcpServerRequest) -> anyhow::Result<()> { self.mcp.update_mcp_server_settings(server_id, request).await }
    async fn stop_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.stop_mcp_retry(server_id).await }
    async fn start_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.start_mcp_retry(server_id).await }
    async fn register_ephemeral_mcp(&self, request: RegisterEphemeralRequest) -> anyhow::Result<usize> { self.mcp.register_ephemeral_mcp(request).await }
    async fn unregister_ephemeral_mcp(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.unregister_ephemeral_mcp(server_id).await }

    // Tool listing/calling — use the composite
    async fn list_all_tools(&self, _ctx: &simply_rpc::RequestContext) -> anyhow::Result<Vec<McpTool>> {
        let defs = self.get_definitions().await;
        Ok(defs.into_iter().map(|d| {
            let schema = serde_json::to_value(&d.input_schema).unwrap_or_default();
            let schema_map = schema.as_object().cloned().unwrap_or_default();
            McpTool::new(d.name, d.description.unwrap_or_default(), schema_map)
        }).collect())
    }

    async fn call_tool_direct(&self, ctx: &simply_rpc::RequestContext, request: CallToolRequestParams) -> anyhow::Result<CallToolResult> {
        let name = request.name.as_ref();
        let args = request.arguments
            .map(serde_json::Value::Object)
            .unwrap_or_default();

        tracing::info!(
            tool = name,
            user_id = ?ctx.scope.user_id,
            "call_tool_direct: dispatching"
        );

        // Get user-scoped or global tool service, then delegate
        let content = if let Some(ref user_id) = ctx.scope.user_id {
            let uid = simply_core::storage::ids::UserId::from_string(user_id);
            match self.for_user_internal(&uid).await {
                Ok(scoped) => scoped.call_tool(name, args).await?,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to build user tools, using global");
                    self.call_tool(name, args).await?
                }
            }
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
