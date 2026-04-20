//! Unified MCP service — owns the registry, OAuth, and auto-connect.
//!
//! Both `EmbeddedDaemon` and the standalone daemon use this.
//! The daemon delegates `McpApi` and `OAuthApi` to this service.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;

use simply_core::McpRegistry;

use simply_rpc::RequestContext;

use crate::api::*;
use crate::mcp::auth::{DaemonMcpConfig, DaemonServerConfig, ServerAuth};
use crate::oauth::OAuthService;

/// Configuration for starting the MCP service.
pub struct McpServiceConfig {
    /// The daemon's public URL — used for OAuth redirect callbacks.
    pub public_url: String,
}

/// Encapsulates all MCP concerns: registry, OAuth, auto-connect.
///
/// `McpRegistry` (simply-core) manages connections — auth-agnostic.
/// `McpService` owns `DaemonMcpConfig` (server list) and resolves
/// bearer tokens before calling into the registry.
pub struct McpService {
    registry: Arc<Mutex<McpRegistry>>,
    /// MCP server configs (mcp.toml).
    daemon_config: Arc<Mutex<DaemonMcpConfig>>,
    /// In-memory scope union declared by skills via OAuthRequirement.
    /// Not persisted — rebuilt on each daemon startup from skill registrations.
    skill_scopes: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    oauth: OAuthService,
}

impl McpService {
    /// Create and start the MCP service.
    pub async fn start(
        config: McpServiceConfig,
        token_store: Arc<crate::token_store::TransientTokenStore>,
    ) -> anyhow::Result<Self> {
        // Ensure ~/.local/share/noema/oauth_providers.toml exists with defaults.
        crate::oauth::providers::ensure_config();

        let daemon_config = crate::mcp::config_io::load_mcp_config();
        let core_config = daemon_config.to_core_config();
        let registry = Arc::new(Mutex::new(McpRegistry::new(core_config)));
        let daemon_config = Arc::new(Mutex::new(daemon_config));
        let skill_scopes = Arc::new(Mutex::new(HashMap::new()));

        let oauth = OAuthService::new(
            Arc::clone(&registry),
            config.public_url,
            Arc::clone(&daemon_config),
            Arc::clone(&skill_scopes),
            token_store,
        );

        // Auto-connect non-OAuth servers globally (OAuth servers connect per-user on demand)
        let static_tokens = daemon_config.lock().await.static_bearer_tokens();
        simply_core::mcp::start_auto_connect(Arc::clone(&registry), static_tokens, None).await;

        Ok(Self { registry, daemon_config, skill_scopes, oauth })
    }

    /// The shared MCP registry — needed by `SessionManager`.
    pub fn registry(&self) -> &Arc<Mutex<McpRegistry>> {
        &self.registry
    }

    /// The OAuth callback redirect URI.
    pub fn oauth_redirect_uri(&self) -> String {
        self.oauth.redirect_uri()
    }

    /// The OAuth callback tracker — share with the axum route handler.
    pub fn oauth_tracker(&self) -> Arc<crate::oauth::callback::CallbackTracker> {
        self.oauth.tracker()
    }

    /// Static bearer token for a server (Token auth only).
    /// OAuth servers return None — they connect per-user via TransientTokenStore.
    async fn static_bearer_token_for(&self, server_id: &str) -> Option<String> {
        let cfg = self.daemon_config.lock().await;
        cfg.get_server(server_id)?.static_bearer_token().map(|t| t.to_string())
    }

    /// Shared daemon config handle — for auth routes and user_tools.
    pub fn daemon_config(&self) -> Arc<Mutex<DaemonMcpConfig>> {
        Arc::clone(&self.daemon_config)
    }

    /// Shared skill scope union — for auth routes to resolve skill-declared scopes.
    pub fn skill_scopes(&self) -> Arc<Mutex<HashMap<String, HashSet<String>>>> {
        Arc::clone(&self.skill_scopes)
    }
}

// ---------------------------------------------------------------------------
// McpApi
// ---------------------------------------------------------------------------

#[async_trait]
impl McpApi for McpService {
    async fn list_mcp_servers(&self) -> anyhow::Result<Vec<McpServerInfo>> {
        let daemon_cfg = self.daemon_config.lock().await;
        let registry = self.registry.lock().await;
        let mut servers = Vec::new();

        for (id, daemon_server) in &daemon_cfg.servers {
            let is_connected = registry.is_connected(id);
            let tool_count = registry.get_connection(id).map(|c| c.tools.len()).unwrap_or(0);
            let status = match registry.get_status(id) {
                simply_core::mcp::ServerStatus::Disconnected => "disconnected".to_string(),
                simply_core::mcp::ServerStatus::Connected => "connected".to_string(),
                simply_core::mcp::ServerStatus::Retrying { attempt } => format!("retrying:{}", attempt),
                simply_core::mcp::ServerStatus::RetryStopped { last_error } => format!("stopped:{}", last_error),
            };
            let (auth_type, needs_oauth_login) = match &daemon_server.auth {
                ServerAuth::None => ("none", false),
                ServerAuth::Token { .. } => ("token", false),
                ServerAuth::OAuth { .. } => ("oauth", true),
            };
            servers.push(McpServerInfo {
                id: id.to_string(),
                name: daemon_server.name.clone(),
                url: daemon_server.url.clone(),
                auth_type: auth_type.to_string(),
                is_connected,
                needs_oauth_login,
                tool_count,
                status,
                auto_connect: daemon_server.auto_connect,
                auto_retry: daemon_server.auto_retry,
            });
        }
        Ok(servers)
    }

    async fn add_mcp_server(&self, request: AddMcpServerRequest) -> anyhow::Result<()> {
        let auth = match request.auth_type.as_str() {
            "token" => ServerAuth::Token {
                token: request.auth_token.unwrap_or_default(),
            },
            "oauth" => {
                let provider_id = request.provider_id
                    .ok_or_else(|| anyhow::anyhow!("oauth auth requires provider_id"))?;
                // Validate provider exists
                if crate::oauth::providers::lookup_provider(&provider_id).is_none() {
                    anyhow::bail!("unknown OAuth provider '{provider_id}' — configure it in oauth_providers.toml first");
                }
                ServerAuth::OAuth {
                    provider_id,
                    scopes: request.scopes.unwrap_or_default(),
                }
            }
            "none" => ServerAuth::None,
            other => anyhow::bail!("unsupported auth_type: {other}"),
        };

        let is_oauth = auth.is_oauth();
        let daemon_server = DaemonServerConfig {
            name: request.name,
            url: request.url.clone(),
            auth,
            auto_connect: !is_oauth,
            auto_retry: !is_oauth,
        };

        let mut registry = self.registry.lock().await;
        registry.add_server(request.id.clone(), daemon_server.core());
        drop(registry);

        let mut daemon_cfg = self.daemon_config.lock().await;
        daemon_cfg.add_server(request.id.clone(), daemon_server);
        crate::mcp::config_io::save_mcp_config(&daemon_cfg)?;
        Ok(())
    }

    async fn remove_mcp_server(&self, server_id: &str) -> anyhow::Result<()> {
        self.registry.lock().await.remove_server(server_id).await?;
        let mut daemon_cfg = self.daemon_config.lock().await;
        daemon_cfg.remove_server(server_id);
        crate::mcp::config_io::save_mcp_config(&daemon_cfg)?;
        Ok(())
    }

    async fn connect_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> {
        let bearer = self.static_bearer_token_for(server_id).await;
        let mut registry = self.registry.lock().await;
        registry.connect(server_id, bearer.as_deref()).await?;
        Ok(registry.get_connection(server_id).map(|c| c.tools.len()).unwrap_or(0))
    }

    async fn disconnect_mcp_server(&self, server_id: &str) -> anyhow::Result<()> {
        self.registry.lock().await.disconnect(server_id).await?;
        Ok(())
    }

    async fn get_mcp_server_tools(&self, server_id: &str) -> anyhow::Result<Vec<McpTool>> {
        let registry = self.registry.lock().await;
        let conn = registry
            .get_connection(server_id)
            .ok_or_else(|| anyhow::anyhow!("server not connected: {server_id}"))?;
        Ok(conn.tools.clone())
    }

    async fn update_mcp_server_settings(
        &self,
        server_id: &str,
        request: UpdateMcpServerRequest,
    ) -> anyhow::Result<()> {
        let mut daemon_cfg = self.daemon_config.lock().await;
        if let Some(server) = daemon_cfg.servers.get_mut(server_id) {
            if let Some(name) = request.name {
                server.name = name;
            }
            if let Some(url) = request.url {
                server.url = url;
            }
            if let Some(auto_connect) = request.auto_connect {
                server.auto_connect = auto_connect;
            }
            if let Some(auto_retry) = request.auto_retry {
                server.auto_retry = auto_retry;
            }
        }
        // Sync name/url changes to registry config
        if let Some(server) = daemon_cfg.servers.get(server_id) {
            let mut registry = self.registry.lock().await;
            registry.add_server(server_id.to_string(), server.core());
        }
        crate::mcp::config_io::save_mcp_config(&daemon_cfg)?;
        Ok(())
    }

    async fn stop_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> {
        self.registry.lock().await.cancel_retry(server_id);
        Ok(())
    }

    async fn start_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> {
        let (config, bearer_token) = {
            let daemon_cfg = self.daemon_config.lock().await;
            let daemon_server = daemon_cfg
                .get_server(server_id)
                .ok_or_else(|| anyhow::anyhow!("server not found: {server_id}"))?;
            (daemon_server.core(), daemon_server.static_bearer_token().map(|t| t.to_string()))
        };
        simply_core::mcp::spawn_retry_task(
            Arc::clone(&self.registry),
            server_id.to_string(),
            config,
            bearer_token,
            None,
        );
        Ok(())
    }

    async fn list_all_tools(&self, _ctx: &RequestContext) -> anyhow::Result<Vec<McpTool>> {
        let registry = self.registry.lock().await;
        let mut tools = Vec::new();
        for (_server_id, server) in registry.connected_servers() {
            tools.extend(server.tools.iter().cloned());
        }
        Ok(tools)
    }

    async fn call_tool_direct(&self, _ctx: &RequestContext, request: CallToolRequestParams) -> anyhow::Result<CallToolResult> {
        let registry = self.registry.lock().await;

        let (tool_caller, arguments) = {
            let mut found = None;
            for (_server_id, server) in registry.connected_servers() {
                if server.tools.iter().any(|t| t.name == request.name) {
                    found = Some((server.tool_caller(), request.arguments.clone()));
                    break;
                }
            }
            found.ok_or_else(|| anyhow::anyhow!("tool not found: {}", request.name))?
        };
        drop(registry);

        let result = tool_caller.call_tool(request.name.to_string(), arguments).await?;
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// OAuthApi
// ---------------------------------------------------------------------------

#[async_trait]
impl OAuthApi for McpService {
    async fn start_oauth(&self, server_id: &str) -> anyhow::Result<OAuthFlowInfo> {
        self.oauth.start_flow(server_id).await
    }

    async fn complete_oauth(&self, server_id: &str, code: &str, state: &str) -> anyhow::Result<()> {
        self.oauth.complete_with_state(server_id, code, state).await
    }

    async fn complete_oauth_with_code(&self, server_id: &str, code: &str) -> anyhow::Result<()> {
        self.oauth.complete_with_code(server_id, code).await
    }

    async fn resolve_oauth_state(&self, state: &str) -> Option<String> {
        self.oauth.resolve_state(state).await
    }

    async fn list_oauth_providers(&self) -> anyhow::Result<Vec<OAuthProviderInfo>> {
        let providers = crate::oauth::providers::load_providers();
        let skill_scopes = self.skill_scopes.lock().await;
        let mut list: Vec<OAuthProviderInfo> = providers.into_iter().map(|(id, p)| {
            let registered_scopes = skill_scopes.get(&id)
                .map(|set| set.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            OAuthProviderInfo {
                id: id.clone(),
                display_name: p.display_name,
                authorization_url: p.authorization_url,
                token_url: p.token_url,
                userinfo_url: p.userinfo_url,
                client_secret_suffix: p.client_secret.as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.chars().rev().take(4).collect::<String>().chars().rev().collect())
                    .unwrap_or_default(),
                client_id: p.client_id,
                has_client_secret: p.client_secret.as_ref().map(|s| !s.is_empty()).unwrap_or(false),
                registered_scopes,
            }
        }).collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(list)
    }

    async fn remove_oauth_provider(&self, provider_id: &str) -> anyhow::Result<()> {
        // Prevent removal if any MCP server still references this provider
        let daemon_cfg = self.daemon_config.lock().await;
        let referencing: Vec<String> = daemon_cfg.servers.iter()
            .filter(|(_, s)| s.auth.oauth_provider_id() == Some(provider_id))
            .map(|(id, _)| id.clone())
            .collect();
        drop(daemon_cfg);
        if !referencing.is_empty() {
            anyhow::bail!(
                "cannot remove provider '{provider_id}' — still referenced by MCP servers: {}",
                referencing.join(", ")
            );
        }

        let mut providers = crate::oauth::providers::load_providers();
        if providers.remove(provider_id).is_none() {
            anyhow::bail!("provider not found: {provider_id}");
        }
        crate::oauth::providers::save_providers(&providers)?;
        self.skill_scopes.lock().await.remove(provider_id);
        tracing::info!(provider_id, "OAuth provider removed");
        Ok(())
    }

    /// Upsert a provider in `oauth_providers.toml`, and merge any declared scopes
    /// into the in-memory union (used at auth time).
    async fn register_oauth_provider(
        &self,
        provider_id: &str,
        request: RegisterOAuthProviderRequest,
    ) -> anyhow::Result<()> {
        // Upsert provider identity — overwrite URLs only if non-empty, preserve creds unless explicitly set
        let mut providers = crate::oauth::providers::load_providers();
        let entry = providers.entry(provider_id.to_string()).or_default();
        if !request.display_name.is_empty() { entry.display_name = request.display_name; }
        if !request.authorization_url.is_empty() { entry.authorization_url = request.authorization_url; }
        if !request.token_url.is_empty() { entry.token_url = request.token_url; }
        if request.userinfo_url.is_some() { entry.userinfo_url = request.userinfo_url; }
        if !request.client_id.is_empty() {
            entry.client_id = request.client_id;
            entry.client_secret = request.client_secret;
        }
        crate::oauth::providers::save_providers(&providers)?;

        // Accumulate skill-declared scopes (in-memory only)
        if !request.scopes.is_empty() {
            self.add_skill_scopes(provider_id, request.scopes).await;
        }

        tracing::debug!(provider_id, "OAuth provider upserted");
        Ok(())
    }
}

impl McpService {
    /// Accumulate skill-declared scopes for a provider (in-memory, not persisted).
    /// Called when a skill registers via `OAuthRequirement`.
    pub async fn add_skill_scopes(&self, provider_id: &str, scopes: impl IntoIterator<Item = String>) {
        let mut map = self.skill_scopes.lock().await;
        let entry = map.entry(provider_id.to_string()).or_default();
        for scope in scopes {
            entry.insert(scope);
        }
    }
}
