//! Per-user tool service cache.
//!
//! Each user gets their own `ToolService` that includes:
//! - Daemon REST tools (same for everyone; role filtering is future)
//! - MCP tools from servers the user can access:
//!   - No-auth servers: included for all users (permission filtering is future)
//!   - OAuth servers: included only if the user has a valid token
//!   - Ephemeral servers (e.g. Lumina Discord tools): always included

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Mutex;

use simply_core::mcp::McpRegistry;
use simply_core::storage::ids::UserId;
use simply_core::{AuthMethod, ToolService};
use llm::{ToolDefinition, ToolResultContent};

use crate::token_store::TransientTokenStore;
use crate::tools::DaemonToolService;

/// Cached per-user tool service.
struct CachedEntry {
    tool_service: Arc<UserToolService>,
    /// Server IDs this cache was built from (for staleness check).
    server_ids: Vec<String>,
}

/// Produces and caches per-user tool services.
pub struct UserToolServiceCache {
    cache: Mutex<HashMap<String, CachedEntry>>,
    daemon_tools: Arc<DaemonToolService>,
    token_store: Arc<TransientTokenStore>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
}

impl UserToolServiceCache {
    pub fn new(
        daemon_tools: Arc<DaemonToolService>,
        token_store: Arc<TransientTokenStore>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
    ) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            daemon_tools,
            token_store,
            mcp_registry,
        }
    }

    /// Get or build the tool service for a user.
    pub async fn get(&self, user_id: &UserId) -> Result<Arc<UserToolService>> {
        let accessible = self.resolve_accessible_servers(user_id).await;

        // Check cache
        {
            let cache = self.cache.lock().await;
            if let Some(entry) = cache.get(user_id.as_str()) {
                if entry.server_ids == accessible {
                    return Ok(Arc::clone(&entry.tool_service));
                }
            }
        }

        // Build fresh
        let mcp_callers = self.build_mcp_callers(user_id, &accessible).await?;
        let svc = Arc::new(UserToolService {
            daemon_tools: Arc::clone(&self.daemon_tools),
            mcp_callers,
        });

        self.cache.lock().await.insert(
            user_id.as_str().to_string(),
            CachedEntry {
                tool_service: Arc::clone(&svc),
                server_ids: accessible,
            },
        );

        Ok(svc)
    }

    /// Invalidate cache for a user (e.g. after they auth with a new MCP server).
    pub async fn invalidate(&self, user_id: &UserId) {
        self.cache.lock().await.remove(user_id.as_str());
    }

    /// Which MCP servers can this user access?
    async fn resolve_accessible_servers(&self, user_id: &UserId) -> Vec<String> {
        let registry = self.mcp_registry.lock().await;
        let mut ids = Vec::new();

        for (id, _server) in registry.connected_servers() {
            let config = registry.config().get_server(id)
                .or_else(|| registry.get_ephemeral(id));

            let include = match config.map(|c| &c.auth) {
                // No auth / static token: everyone gets it (role filtering is future)
                Some(AuthMethod::None) | Some(AuthMethod::Token { .. }) | None => true,
                // OAuth: only if user has a token
                Some(AuthMethod::OAuth { .. }) => self.token_store.has_token(user_id, id),
            };

            if include {
                ids.push(id.to_string());
            }
        }

        ids.sort();
        ids
    }

    /// Build MCP callers for the given server IDs.
    async fn build_mcp_callers(
        &self,
        user_id: &UserId,
        server_ids: &[String],
    ) -> Result<Vec<McpCaller>> {
        // Phase 1: under lock, collect global callers and configs for per-user connections
        let (global_callers, user_configs) = {
            let registry = self.mcp_registry.lock().await;
            let mut global = Vec::new();
            let mut needs_user_conn = Vec::new();

            for id in server_ids {
                let config = registry.config().get_server(id)
                    .or_else(|| registry.get_ephemeral(id));

                let is_oauth = config
                    .map(|c| matches!(c.auth, AuthMethod::OAuth { .. }))
                    .unwrap_or(false);

                if is_oauth {
                    if let (Some(cfg), Some(token)) = (config, self.token_store.get(user_id, id)) {
                        let mut user_cfg = cfg.clone();
                        user_cfg.auth = AuthMethod::Token { token: token.access_token };
                        needs_user_conn.push((id.clone(), user_cfg));
                    }
                } else if let Some(connected) = registry.get_connection(id) {
                    global.push(McpCaller::from_connected(connected));
                }
            }

            (global, needs_user_conn)
        };
        // Lock released here

        // Phase 2: connect to OAuth servers with per-user tokens (no lock held)
        let mut callers = global_callers;
        for (id, config) in user_configs {
            match McpRegistry::connect_to_server(&config).await {
                Ok(connected) => {
                    callers.push(McpCaller::from_connected(&connected));
                }
                Err(e) => {
                    tracing::warn!(server_id = %id, error = %e, "per-user MCP connection failed");
                }
            }
        }

        Ok(callers)
    }
}

/// A single MCP server's tool caller + definitions.
struct McpCaller {
    tools: Vec<ToolDefinition>,
    caller: simply_core::mcp::McpToolCaller,
}

impl McpCaller {
    fn from_connected(connected: &simply_core::mcp::ConnectedServer) -> Self {
        let tools = connected.tools.iter().map(|t| {
            let schema = serde_json::Value::Object((*t.input_schema).clone());
            ToolDefinition {
                name: t.name.to_string(),
                description: t.description.as_ref().map(|s| s.to_string()),
                input_schema: serde_json::from_value(schema).unwrap_or_default(),
            }
        }).collect();
        Self {
            tools,
            caller: connected.tool_caller(),
        }
    }
}

/// Per-user tool service: daemon tools + user-specific MCP tools.
pub struct UserToolService {
    daemon_tools: Arc<DaemonToolService>,
    mcp_callers: Vec<McpCaller>,
}

#[async_trait]
impl ToolService for UserToolService {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = self.daemon_tools.get_definitions().await;
        for caller in &self.mcp_callers {
            defs.extend(caller.tools.iter().cloned());
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

        // Try MCP callers
        let args_map = match arguments {
            serde_json::Value::Object(map) => Some(map),
            _ => None,
        };

        for caller in &self.mcp_callers {
            if caller.tools.iter().any(|t| t.name == name) {
                let result = caller.caller.call_tool(name.to_string(), args_map).await?;
                let content: Vec<ToolResultContent> = result.content.into_iter().map(|c| {
                    match c.raw {
                        rmcp::model::RawContent::Text(t) => ToolResultContent::text(t.text),
                        rmcp::model::RawContent::Image(img) => {
                            ToolResultContent::image(img.data, img.mime_type)
                        }
                        _ => ToolResultContent::text("[unsupported content type]"),
                    }
                }).collect();
                return Ok(content);
            }
        }

        anyhow::bail!("tool not found: {name}")
    }
}
