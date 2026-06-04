//! ToolRegistry — unified dispatch across all tool providers.
//!
//! Replaces the old CompositeToolService. Daemon REST tools + a list of
//! non-MCP ToolProviders (WS skills, embedded skills) + MCP servers looked
//! up dynamically from `McpService` so newly-connected servers are picked
//! up automatically.
//! Also implements McpApi by delegating management to McpService.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use simply_core::mcp::McpToolCaller;
use simply_rpc::{RequestContext, RestService};
use tokio::sync::RwLock;

use simply_core::ToolService;
use simply_daemon_api::{ProviderKind, ToolProvider};

use crate::api::*;
use crate::mcp::McpService;
use super::tools::DaemonToolService;

/// Newly-minted token from a refresh-token exchange.
struct RefreshedToken {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
}

/// Exchange a refresh token for a fresh access token at the provider's token endpoint.
async fn refresh_access_token(
    provider: &crate::oauth::providers::OAuthProvider,
    refresh_token: &str,
) -> anyhow::Result<RefreshedToken> {
    let client = reqwest::Client::new();
    let mut params: Vec<(&str, &str)> = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", provider.client_id.as_str()),
    ];
    let secret;
    if let Some(s) = &provider.client_secret {
        secret = s.clone();
        params.push(("client_secret", &secret));
    }
    let resp = client.post(&provider.token_url).form(&params).send().await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("refresh token exchange failed: {body}");
    }
    #[derive(serde::Deserialize)]
    struct Resp {
        access_token: String,
        expires_in: Option<u64>,
        refresh_token: Option<String>,
    }
    let r: Resp = resp.json().await?;
    Ok(RefreshedToken {
        access_token: r.access_token,
        expires_in: r.expires_in,
        refresh_token: r.refresh_token,
    })
}

/// Unified tool registry — all tool providers in one place.
pub struct ToolRegistry {
    /// Daemon's own REST API methods exposed as tools.
    daemon_tools: Arc<DaemonToolService>,
    /// Non-MCP tool providers (WS skills, embedded skills). MCP servers are
    /// sourced live from `McpService` so async (re)connects are reflected.
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

    /// Augment a caller-supplied context with OAuth tokens for the ctx's user.
    ///
    /// Pulls tokens from the TransientTokenStore for:
    /// - Each MCP server (keyed by server_id — legacy Token-auth servers)
    /// - Each OAuth provider (keyed by provider_id — what skills and OAuth
    ///   MCP servers both look up)
    ///
    /// Everything else on the ctx (scope, pre-existing tokens, metadata) is
    /// preserved — so origin hints like `discord.guild_id` set at session
    /// creation flow through to tool handlers.
    pub async fn augment_with_tokens(&self, mut ctx: RequestContext) -> RequestContext {
        let Some(uid_str) = ctx.scope.user_id.clone() else { return ctx; };
        let user_id = simply_core::storage::ids::UserId::from_string(&uid_str);

        {
            let registry = self.mcp.registry().lock().await;
            for (server_id, _) in registry.config().servers.iter() {
                if !ctx.tokens.contains_key(server_id) {
                    if let Some(token) = self.token_store.get(&user_id, server_id) {
                        ctx.tokens.insert(server_id.clone(), token.access_token);
                    }
                }
            }
        }

        // OAuth providers (used by skills and OAuth MCP servers) — tokens keyed
        // by provider_id. Refreshes an expired access token from its refresh
        // token when possible, so persisted logins keep working after expiry.
        let known = crate::oauth::providers::known_providers();
        for provider_id in &known {
            if !ctx.tokens.contains_key(provider_id) {
                if let Some(access) = self.resolve_access_token(&user_id, provider_id).await {
                    ctx.tokens.insert(provider_id.clone(), access);
                }
            }
        }

        tracing::debug!(
            user_id = %uid_str,
            known_providers = ?known,
            token_keys = ?ctx.tokens.keys().collect::<Vec<_>>(),
            metadata_keys = ?ctx.metadata.keys().collect::<Vec<_>>(),
            "augment_with_tokens"
        );

        ctx
    }

    /// Resolve a usable access token for (user, provider): return the stored one
    /// if still valid, otherwise refresh it using the stored refresh token.
    /// Returns None if there's no token, no refresh token, or the refresh fails.
    async fn resolve_access_token(
        &self,
        user_id: &simply_core::storage::ids::UserId,
        provider_id: &str,
    ) -> Option<String> {
        let token = self.token_store.get_raw(user_id, provider_id)?;
        if !token.is_expired() {
            return Some(token.access_token);
        }
        let refresh = token.refresh_token.clone()?;
        let provider = crate::oauth::providers::lookup_provider(provider_id)?;
        match refresh_access_token(&provider, &refresh).await {
            Ok(r) => {
                self.token_store.store(
                    user_id,
                    provider_id,
                    crate::token_store::McpUserToken {
                        access_token: r.access_token.clone(),
                        // Google usually omits refresh_token on refresh — keep ours.
                        refresh_token: r.refresh_token.or(token.refresh_token),
                        expires_at: r.expires_in.map(|s| crate::token_store::now_unix() + s as i64),
                        identity: token.identity,
                    },
                );
                tracing::info!(provider_id, user_id = %user_id, "refreshed expired OAuth access token");
                Some(r.access_token)
            }
            Err(e) => {
                tracing::warn!(provider_id, error = %e, "OAuth token refresh failed");
                None
            }
        }
    }

    /// Get a ToolService bound to a specific request context (user + tokens +
    /// origin metadata). Used by sessions so tool calls carry the caller's
    /// ambient context (e.g. `discord.guild_id`) through to skills.
    pub async fn for_ctx(self: &Arc<Self>, base_ctx: RequestContext) -> Arc<dyn ToolService> {
        let ctx = self.augment_with_tokens(base_ctx).await;
        let scoped_daemon = Arc::new(self.daemon_tools.with_context(ctx.clone()));
        Arc::new(UserScopedTools { registry: Arc::clone(self), scoped_daemon, ctx })
    }

    /// Snapshot of currently-connected MCP tools: (tool, caller) pairs.
    ///
    /// Queried live so servers that connect (or reconnect) after startup
    /// are picked up automatically — mirrors how skills/WS providers are
    /// mutated at runtime via `register`/`unregister`.
    async fn mcp_tools_snapshot(&self) -> Vec<(Tool, McpToolCaller)> {
        let registry = self.mcp.registry().lock().await;
        let mut out = Vec::new();
        for (_id, conn) in registry.connected_servers() {
            let caller = conn.tool_caller();
            for tool in &conn.tools {
                out.push((tool.clone(), caller.clone()));
            }
        }
        out
    }

    /// List all tool definitions — daemon tools + registered providers + live MCP.
    async fn list_definitions(&self, daemon_tools: &DaemonToolService) -> Vec<ToolDefinition> {
        let mut defs = daemon_tools.get_definitions().await;
        for provider in self.providers.read().await.iter() {
            for tool in provider.tools().await {
                defs.push(mcp_tool_to_definition(&tool));
            }
        }
        for (tool, _) in self.mcp_tools_snapshot().await {
            defs.push(mcp_tool_to_definition(&tool));
        }
        defs
    }

    /// Dispatch a tool call against daemon tools, static providers, then live MCP.
    async fn dispatch(
        &self,
        daemon_tools: &DaemonToolService,
        name: &str,
        arguments: serde_json::Value,
        ctx: &RequestContext,
    ) -> Result<Vec<ToolResultContent>> {
        if daemon_tools.get_definitions().await.iter().any(|d| d.name == name) {
            return daemon_tools.call_tool(name, arguments).await;
        }
        let request = CallToolRequestParams::new(name.to_string())
            .with_arguments(arguments.as_object().cloned().unwrap_or_default());
        for provider in self.providers.read().await.iter() {
            if provider.tools().await.iter().any(|t| t.name.as_ref() == name) {
                let result = provider.call_tool(request.clone(), ctx).await?;
                return Ok(call_result_to_content(result));
            }
        }
        for (tool, caller) in self.mcp_tools_snapshot().await {
            if tool.name.as_ref() == name {
                let args = arguments.as_object().cloned();
                let result = caller.call_tool(name.to_string(), args).await?;
                return Ok(call_result_to_content(result));
            }
        }
        anyhow::bail!("tool not found: {name}")
    }
}

// ---------------------------------------------------------------------------
// ToolService — dispatches across daemon REST tools + providers + live MCP
// ---------------------------------------------------------------------------

#[async_trait]
impl ToolService for ToolRegistry {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.list_definitions(&self.daemon_tools).await
    }

    async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<Vec<ToolResultContent>> {
        let ctx = RequestContext::anonymous();
        self.dispatch(&self.daemon_tools, name, arguments, &ctx).await
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

        // Augment the caller's ctx with any OAuth tokens we hold for them;
        // metadata and pre-existing tokens are preserved.
        let scoped_ctx = self.augment_with_tokens(ctx.clone()).await;
        let scoped_daemon = self.daemon_tools.with_context(scoped_ctx.clone());
        let content = self.dispatch(&scoped_daemon, name, args, &scoped_ctx).await?;

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
// UserScopedTools — per-session wrapper over ToolRegistry with user context
// ---------------------------------------------------------------------------

struct UserScopedTools {
    registry: Arc<ToolRegistry>,
    scoped_daemon: Arc<DaemonToolService>,
    ctx: RequestContext,
}

#[async_trait]
impl ToolService for UserScopedTools {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.registry.list_definitions(&self.scoped_daemon).await
    }

    async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<Vec<ToolResultContent>> {
        self.registry.dispatch(&self.scoped_daemon, name, arguments, &self.ctx).await
    }
}

// ---------------------------------------------------------------------------
// SkillsApi — read-only listing of registered skills
// ---------------------------------------------------------------------------

#[async_trait]
impl SkillsApi for ToolRegistry {
    async fn list_skills(&self) -> anyhow::Result<Vec<SkillInfo>> {
        let providers = self.providers.read().await;
        let mut skills = Vec::new();
        for provider in providers.iter() {
            let kind = match provider.kind() {
                ProviderKind::InProcess => "in-process",
                ProviderKind::Remote => "remote",
                ProviderKind::Mcp => continue,
            };
            let oauth_provider_ids = provider.oauth_requirements().await
                .into_iter().map(|r| r.provider_id).collect();
            skills.push(SkillInfo {
                id: provider.id().to_string(),
                display_name: provider.display_name().to_string(),
                kind: kind.to_string(),
                tool_count: provider.tools().await.len(),
                is_connected: provider.is_connected(),
                oauth_provider_ids,
            });
        }
        skills.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(skills)
    }

    async fn list_skill_tools(&self, skill_id: &str) -> anyhow::Result<Vec<McpTool>> {
        let providers = self.providers.read().await;
        for provider in providers.iter() {
            if provider.kind() == ProviderKind::Mcp { continue; }
            if provider.id() == skill_id {
                return Ok(provider.tools().await);
            }
        }
        anyhow::bail!("skill not found: {skill_id}")
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
