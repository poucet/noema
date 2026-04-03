//! Unified MCP service — owns the registry, OAuth, daemon MCP server, and auto-connect.
//!
//! Both `EmbeddedDaemon` and the future standalone daemon use this.
//! The daemon delegates `McpApi` and `OAuthApi` to this service.

use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;

use simply_core::McpRegistry;
use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::traits::StorageTypes;
use simply_core::storage::DocumentResolver;

use crate::api::*;
use crate::mcp::{DaemonMcpServer, ServerHandle, start_server};
use crate::oauth::OAuthService;

/// Configuration for starting the MCP service.
pub struct McpServiceConfig {
    pub oauth_callback_port: Option<u16>,
}

/// Encapsulates all MCP concerns: registry, OAuth, daemon tools server, auto-connect.
pub struct McpService {
    registry: Arc<Mutex<McpRegistry>>,
    oauth: OAuthService,
    _daemon_server: ServerHandle,
}

impl McpService {
    /// Create and start the MCP service.
    ///
    /// - Loads the MCP registry from config
    /// - Starts the daemon's own MCP server (exposes tools like spawn_agent)
    /// - Registers and connects the daemon server in the registry
    /// - Starts the OAuth callback server
    /// - Auto-connects configured MCP servers in background
    pub async fn start<S: StorageTypes>(
        coordinator: &Arc<StorageCoordinator<S>>,
        document_resolver: Arc<dyn DocumentResolver>,
        config: McpServiceConfig,
    ) -> anyhow::Result<Self>
    where
        S::Document: DocumentResolver,
    {
        // Load registry
        let registry = Arc::new(Mutex::new(
            McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default())),
        ));

        // Start daemon's own MCP server
        let mcp_server = DaemonMcpServer::new(
            Arc::clone(coordinator),
            Arc::clone(&registry),
            document_resolver,
        );
        let server_handle = start_server(mcp_server).await?;
        let server_url = server_handle.url();

        // Register daemon tools in the registry
        {
            let mut reg = registry.lock().await;
            reg.register_ephemeral("daemon-tools".to_string(), server_url);
            reg.connect("daemon-tools").await?;
        }

        // Start OAuth callback server
        let oauth = OAuthService::start(Arc::clone(&registry), config.oauth_callback_port).await?;

        // Auto-connect configured servers in background
        simply_core::mcp::start_auto_connect(Arc::clone(&registry), None).await;

        Ok(Self {
            registry,
            oauth,
            _daemon_server: server_handle,
        })
    }

    /// The shared MCP registry — needed by `SessionManager`.
    pub fn registry(&self) -> &Arc<Mutex<McpRegistry>> {
        &self.registry
    }

    /// The OAuth callback redirect URI.
    pub fn oauth_redirect_uri(&self) -> String {
        self.oauth.redirect_uri()
    }

}

// ---------------------------------------------------------------------------
// McpApi
// ---------------------------------------------------------------------------

#[async_trait]
impl McpApi for McpService {
    async fn list_mcp_servers(&self) -> anyhow::Result<Vec<McpServerInfo>> {
        let registry = self.registry.lock().await;
        let mut servers = Vec::new();
        for (id, config) in registry.list_servers() {
            let is_connected = registry.is_connected(id);
            let tool_count = registry
                .get_connection(id)
                .map(|c| c.tools.len())
                .unwrap_or(0);
            let status = match registry.get_status(id) {
                simply_core::mcp::ServerStatus::Disconnected => "disconnected".to_string(),
                simply_core::mcp::ServerStatus::Connected => "connected".to_string(),
                simply_core::mcp::ServerStatus::Retrying { attempt } => {
                    format!("retrying:{}", attempt)
                }
                simply_core::mcp::ServerStatus::RetryStopped { last_error } => {
                    format!("stopped:{}", last_error)
                }
            };
            let auth_type = match &config.auth {
                simply_core::AuthMethod::None => "none",
                simply_core::AuthMethod::Token { .. } => "token",
                simply_core::AuthMethod::OAuth { .. } => "oauth",
            };
            servers.push(McpServerInfo {
                id: id.to_string(),
                name: config.name.clone(),
                url: config.url.clone(),
                auth_type: auth_type.to_string(),
                is_connected,
                needs_oauth_login: config.auth.needs_oauth_login(),
                tool_count,
                status,
                auto_connect: config.auto_connect,
                auto_retry: config.auto_retry,
            });
        }
        Ok(servers)
    }

    async fn add_mcp_server(&self, request: AddMcpServerRequest) -> anyhow::Result<()> {
        let auth = match request.auth_type.as_str() {
            "token" => simply_core::AuthMethod::Token {
                token: request.auth_token.unwrap_or_default(),
            },
            "oauth" => simply_core::AuthMethod::OAuth {
                client_id: request.client_id.unwrap_or_else(|| "simply".to_string()),
                client_secret: request.client_secret,
                authorization_url: None,
                token_url: None,
                scopes: request.scopes.unwrap_or_default(),
                access_token: None,
                refresh_token: None,
                expires_at: None,
            },
            "none" => simply_core::AuthMethod::None,
            _ => {
                tracing::info!(url = %request.url, "auto-detecting auth via .well-known");
                if let Ok(well_known) = crate::oauth::fetch_well_known(&request.url).await {
                    if well_known.get("authorization_endpoint").is_some() {
                        tracing::info!("OAuth detected via .well-known");
                        simply_core::AuthMethod::OAuth {
                            client_id: "simply".to_string(),
                            client_secret: None,
                            authorization_url: None,
                            token_url: None,
                            scopes: vec![],
                            access_token: None,
                            refresh_token: None,
                            expires_at: None,
                        }
                    } else {
                        simply_core::AuthMethod::None
                    }
                } else {
                    simply_core::AuthMethod::None
                }
            }
        };

        let use_well_known = matches!(auth, simply_core::AuthMethod::OAuth { .. });
        let config = simply_core::ServerConfig {
            name: request.name,
            url: request.url,
            auth,
            auth_token: None,
            auto_connect: true,
            auto_retry: true,
            use_well_known,
        };
        let mut registry = self.registry.lock().await;
        registry.add_server(request.id.clone(), config);
        registry.save_config()?;
        Ok(())
    }

    async fn remove_mcp_server(&self, server_id: &str) -> anyhow::Result<()> {
        let mut registry = self.registry.lock().await;
        registry.remove_server(server_id).await?;
        registry.save_config()?;
        Ok(())
    }

    async fn connect_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> {
        let mut registry = self.registry.lock().await;
        registry.connect(server_id).await?;
        Ok(registry
            .get_connection(server_id)
            .map(|c| c.tools.len())
            .unwrap_or(0))
    }

    async fn disconnect_mcp_server(&self, server_id: &str) -> anyhow::Result<()> {
        let mut registry = self.registry.lock().await;
        registry.disconnect(server_id).await?;
        Ok(())
    }

    async fn get_mcp_server_tools(
        &self,
        server_id: &str,
    ) -> anyhow::Result<Vec<McpToolInfo>> {
        let registry = self.registry.lock().await;
        let conn = registry
            .get_connection(server_id)
            .ok_or_else(|| anyhow::anyhow!("server not connected: {server_id}"))?;
        Ok(conn
            .tools
            .iter()
            .map(|t| McpToolInfo {
                name: t.name.to_string(),
                description: t.description.as_deref().map(|s| s.to_string()),
                server_id: server_id.to_string(),
            })
            .collect())
    }

    async fn test_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> {
        let mut registry = self.registry.lock().await;
        let _ = registry.disconnect(server_id).await;
        registry.connect(server_id).await?;
        Ok(registry
            .get_connection(server_id)
            .map(|c| c.tools.len())
            .unwrap_or(0))
    }

    async fn update_mcp_server_settings(
        &self,
        server_id: &str,
        request: UpdateMcpServerRequest,
    ) -> anyhow::Result<()> {
        let mut registry = self.registry.lock().await;
        if let Some(config) = registry.config_mut().servers.get_mut(server_id) {
            if let Some(name) = request.name {
                config.name = name;
            }
            if let Some(url) = request.url {
                config.url = url;
            }
            if let Some(auto_connect) = request.auto_connect {
                config.auto_connect = auto_connect;
            }
            if let Some(auto_retry) = request.auto_retry {
                config.auto_retry = auto_retry;
            }
        }
        registry.save_config()?;
        Ok(())
    }

    async fn stop_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> {
        let mut registry = self.registry.lock().await;
        registry.cancel_retry(server_id);
        Ok(())
    }

    async fn start_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> {
        let config = {
            let registry = self.registry.lock().await;
            registry
                .config()
                .servers
                .get(server_id)
                .ok_or_else(|| anyhow::anyhow!("server not found: {server_id}"))?
                .clone()
        };
        simply_core::mcp::spawn_retry_task(
            Arc::clone(&self.registry),
            server_id.to_string(),
            config,
            None,
        );
        Ok(())
    }

    async fn register_ephemeral_mcp(&self, request: RegisterEphemeralRequest) -> anyhow::Result<usize> {
        let mut registry = self.registry.lock().await;
        registry.register_ephemeral(request.id.clone(), request.url);
        registry.connect(&request.id).await?;
        let tool_count = registry
            .get_connection(&request.id)
            .map(|c| c.tools.len())
            .unwrap_or(0);
        tracing::info!(id = %request.id, tool_count, "ephemeral MCP service registered");
        Ok(tool_count)
    }

    async fn unregister_ephemeral_mcp(&self, server_id: &str) -> anyhow::Result<()> {
        let mut registry = self.registry.lock().await;
        registry.unregister_ephemeral(server_id).await;
        tracing::info!(id = %server_id, "ephemeral MCP service unregistered");
        Ok(())
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

    async fn complete_oauth(
        &self,
        server_id: &str,
        code: &str,
        state: &str,
    ) -> anyhow::Result<()> {
        self.oauth.complete_with_state(server_id, code, state).await
    }

    async fn complete_oauth_with_code(
        &self,
        server_id: &str,
        code: &str,
    ) -> anyhow::Result<()> {
        self.oauth.complete_with_code(server_id, code).await
    }

    async fn resolve_oauth_state(&self, state: &str) -> Option<String> {
        self.oauth.resolve_state(state).await
    }
}
