use crate::mcp::config::{McpConfig, ServerConfig};
use crate::traffic_log;
use anyhow::Result;
use llm::{ToolDefinition, ToolResultContent};
use rmcp::{
    model::{CallToolRequestParam, RawContent, Tool},
    service::{Peer, RunningService},
    transport::streamable_http_client::{
        StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
    },
    RoleClient, ServiceExt,
};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// A cloneable handle for calling MCP tools without holding registry locks.
///
/// This is a lightweight wrapper around the rmcp Peer that can be cloned
/// and used to make tool calls while not holding locks on the registry.
#[derive(Clone)]
pub struct McpToolCaller {
    peer: Peer<RoleClient>,
}

impl McpToolCaller {
    /// Call a tool on this server
    pub async fn call_tool(
        &self,
        name: String,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<rmcp::model::CallToolResult> {
        let result = self
            .peer
            .call_tool(CallToolRequestParam {
                name: name.into(),
                arguments,
            })
            .await?;
        Ok(result)
    }
}

/// A connected MCP server with its available tools.
pub struct ConnectedServer {
    pub config: ServerConfig,
    pub tools: Vec<Tool>,
    service: RunningService<rmcp::RoleClient, ()>,
}

impl ConnectedServer {
    /// Get a cloneable tool caller that can be used without holding registry locks.
    ///
    /// Use this when you need to make tool calls while releasing the registry lock.
    pub fn tool_caller(&self) -> McpToolCaller {
        McpToolCaller {
            peer: self.service.deref().clone(),
        }
    }

    /// Call a tool on this server
    pub async fn call_tool(
        &self,
        name: String,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<rmcp::model::CallToolResult> {
        let result = self
            .service
            .call_tool(CallToolRequestParam {
                name: name.into(),
                arguments,
            })
            .await?;
        Ok(result)
    }

    /// Disconnect from the server
    pub async fn disconnect(self) -> Result<()> {
        self.service.cancel().await?;
        Ok(())
    }
}

/// Status of a server's connection/retry state
#[derive(Debug, Clone, PartialEq)]
pub enum ServerStatus {
    /// Not connected, no retry in progress
    Disconnected,
    /// Connected successfully
    Connected,
    /// Retry in progress with current attempt number
    Retrying { attempt: u32 },
    /// Retry stopped (manually or max retries reached)
    RetryStopped { last_error: String },
}

/// Registry managing MCP server connections.
pub struct McpRegistry {
    config: McpConfig,
    connections: HashMap<String, ConnectedServer>,
    /// Cancellation tokens for active retry tasks
    retry_tokens: HashMap<String, CancellationToken>,
    /// Current status of each server
    server_status: HashMap<String, ServerStatus>,
}

impl McpRegistry {
    /// Create a new registry with the given configuration
    pub fn new(config: McpConfig) -> Self {
        Self {
            config,
            connections: HashMap::new(),
            retry_tokens: HashMap::new(),
            server_status: HashMap::new(),
        }
    }

    /// Load configuration and create a new registry
    pub fn load() -> Result<Self> {
        let config = McpConfig::load()?;
        Ok(Self::new(config))
    }

    /// Get the current configuration
    pub fn config(&self) -> &McpConfig {
        &self.config
    }

    /// Get a mutable reference to the configuration
    pub fn config_mut(&mut self) -> &mut McpConfig {
        &mut self.config
    }

    /// List all configured servers
    pub fn list_servers(&self) -> Vec<(&str, &ServerConfig)> {
        self.config
            .servers
            .iter()
            .map(|(id, cfg)| (id.as_str(), cfg))
            .collect()
    }

    /// Check if a server is connected
    pub fn is_connected(&self, id: &str) -> bool {
        self.connections.contains_key(id)
    }

    /// Get a connected server
    pub fn get_connection(&self, id: &str) -> Option<&ConnectedServer> {
        self.connections.get(id)
    }

    /// Connect to a configured server
    pub async fn connect(&mut self, id: &str) -> Result<&ConnectedServer> {
        if self.connections.contains_key(id) {
            return Ok(self.connections.get(id).unwrap());
        }

        let server_config = self
            .config
            .get_server(id)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in configuration", id))?
            .clone();

        let connected = Self::connect_to_server(&server_config).await?;
        self.connections.insert(id.to_string(), connected);
        Ok(self.connections.get(id).unwrap())
    }

    /// Connect to a server configuration (public for retry task access)
    pub async fn connect_to_server(config: &ServerConfig) -> Result<ConnectedServer> {
        // Get bearer token from auth method (new) or legacy auth_token field
        let bearer_token = config.auth.bearer_token().or(config.auth_token.as_deref());

        let transport = if let Some(token) = bearer_token {
            let mut transport_config =
                StreamableHttpClientTransportConfig::with_uri(Arc::from(config.url.as_str()));
            transport_config = transport_config.auth_header(token.to_string());
            StreamableHttpClientTransport::from_config(transport_config)
        } else {
            StreamableHttpClientTransport::from_uri(config.url.as_str())
        };

        let service = ().serve(transport).await?;

        let tools_result = service.list_tools(Default::default()).await?;

        Ok(ConnectedServer {
            config: config.clone(),
            tools: tools_result.tools,
            service,
        })
    }

    /// Disconnect from a server
    pub async fn disconnect(&mut self, id: &str) -> Result<()> {
        if let Some(connection) = self.connections.remove(id) {
            connection.disconnect().await?;
        }
        Ok(())
    }

    /// Disconnect from all servers
    pub async fn disconnect_all(&mut self) -> Result<()> {
        let ids: Vec<String> = self.connections.keys().cloned().collect();
        for id in ids {
            self.disconnect(&id).await?;
        }
        Ok(())
    }

    /// Add a new server to the configuration
    pub fn add_server(&mut self, id: String, config: ServerConfig) {
        self.config.add_server(id, config);
    }

    /// Remove a server from the configuration (disconnects if connected)
    pub async fn remove_server(&mut self, id: &str) -> Result<Option<ServerConfig>> {
        self.disconnect(id).await?;
        Ok(self.config.remove_server(id))
    }

    /// Save the current configuration
    pub fn save_config(&self) -> Result<()> {
        self.config.save()
    }

    /// Get all connected servers
    pub fn connected_servers(&self) -> impl Iterator<Item = (&str, &ConnectedServer)> {
        self.connections.iter().map(|(id, s)| (id.as_str(), s))
    }

    /// Get the status of a server
    pub fn get_status(&self, id: &str) -> ServerStatus {
        self.server_status
            .get(id)
            .cloned()
            .unwrap_or(if self.connections.contains_key(id) {
                ServerStatus::Connected
            } else {
                ServerStatus::Disconnected
            })
    }

    /// Get all server statuses
    pub fn all_statuses(&self) -> HashMap<String, ServerStatus> {
        let mut statuses = HashMap::new();
        for (id, _) in &self.config.servers {
            statuses.insert(id.clone(), self.get_status(id));
        }
        statuses
    }

    /// Update server status
    pub fn set_status(&mut self, id: &str, status: ServerStatus) {
        self.server_status.insert(id.to_string(), status);
    }

    /// Check if a retry is active for a server
    pub fn is_retry_active(&self, id: &str) -> bool {
        self.retry_tokens.contains_key(id)
    }

    /// Store a retry cancellation token
    pub fn set_retry_token(&mut self, id: &str, token: CancellationToken) {
        self.retry_tokens.insert(id.to_string(), token);
    }

    /// Cancel and remove a retry task
    pub fn cancel_retry(&mut self, id: &str) {
        if let Some(token) = self.retry_tokens.remove(id) {
            token.cancel();
        }
    }

    /// Remove retry token (when retry completes naturally)
    pub fn remove_retry_token(&mut self, id: &str) {
        self.retry_tokens.remove(id);
    }

    /// Store a successful connection (called from retry task)
    pub fn store_connection(&mut self, id: &str, server: ConnectedServer) {
        self.connections.insert(id.to_string(), server);
        self.server_status.insert(id.to_string(), ServerStatus::Connected);
        self.retry_tokens.remove(id);
    }

    /// Get servers that should auto-connect
    pub fn auto_connect_servers(&self) -> Vec<(String, ServerConfig)> {
        self.config
            .servers
            .iter()
            .filter(|(_, cfg)| cfg.auto_connect)
            .map(|(id, cfg)| (id.clone(), cfg.clone()))
            .collect()
    }
}

/// Constants for exponential backoff
const INITIAL_BACKOFF_MS: u64 = 1000; // 1 second
const MAX_BACKOFF_MS: u64 = 60000; // 1 minute
const BACKOFF_MULTIPLIER: f64 = 2.0;

/// Spawn a background retry task for connecting to an MCP server.
/// Returns a cancellation token that can be used to stop the retry loop.
pub fn spawn_retry_task(
    registry: Arc<Mutex<McpRegistry>>,
    server_id: String,
    config: ServerConfig,
    on_status_change: Option<Box<dyn Fn(&str, &ServerStatus) + Send + Sync>>,
) -> CancellationToken {
    let token = CancellationToken::new();
    let cancel_token = token.clone();

    tokio::spawn(async move {
        let mut attempt: u32 = 0;
        let mut backoff_ms = INITIAL_BACKOFF_MS;

        loop {
            attempt += 1;

            // Update status to retrying
            {
                let mut reg = registry.lock().await;
                reg.set_status(&server_id, ServerStatus::Retrying { attempt });
                if let Some(ref cb) = on_status_change {
                    cb(&server_id, &ServerStatus::Retrying { attempt });
                }
            }

            // Try to connect
            match McpRegistry::connect_to_server(&config).await {
                Ok(connected) => {
                    // Success! Store connection and exit
                    let mut reg = registry.lock().await;
                    reg.store_connection(&server_id, connected);
                    if let Some(ref cb) = on_status_change {
                        cb(&server_id, &ServerStatus::Connected);
                    }
                    tracing::info!("MCP server '{}' connected after {} attempts", server_id, attempt);
                    return;
                }
                Err(e) => {
                    tracing::warn!(
                        "MCP server '{}' connection attempt {} failed: {}",
                        server_id,
                        attempt,
                        e
                    );

                    // Check if auto_retry is still enabled
                    let should_retry = {
                        let reg = registry.lock().await;
                        reg.config()
                            .get_server(&server_id)
                            .map(|c| c.auto_retry)
                            .unwrap_or(false)
                    };

                    if !should_retry {
                        let mut reg = registry.lock().await;
                        let status = ServerStatus::RetryStopped {
                            last_error: e.to_string(),
                        };
                        reg.set_status(&server_id, status.clone());
                        reg.remove_retry_token(&server_id);
                        if let Some(ref cb) = on_status_change {
                            cb(&server_id, &status);
                        }
                        return;
                    }

                    // Wait with exponential backoff, checking for cancellation
                    tokio::select! {
                        _ = cancel_token.cancelled() => {
                            let mut reg = registry.lock().await;
                            let status = ServerStatus::RetryStopped {
                                last_error: "Retry cancelled".to_string(),
                            };
                            reg.set_status(&server_id, status.clone());
                            reg.remove_retry_token(&server_id);
                            if let Some(ref cb) = on_status_change {
                                cb(&server_id, &status);
                            }
                            return;
                        }
                        _ = tokio::time::sleep(Duration::from_millis(backoff_ms)) => {
                            // Increase backoff for next attempt
                            backoff_ms = ((backoff_ms as f64) * BACKOFF_MULTIPLIER) as u64;
                            backoff_ms = backoff_ms.min(MAX_BACKOFF_MS);
                        }
                    }
                }
            }
        }
    });

    token
}

/// Start auto-connect for all configured servers that have auto_connect enabled.
/// Returns the number of servers that started connecting.
pub async fn start_auto_connect(
    registry: Arc<Mutex<McpRegistry>>,
    on_status_change: Option<Arc<dyn Fn(&str, &ServerStatus) + Send + Sync>>,
) -> usize {
    let servers_to_connect: Vec<(String, ServerConfig)> = {
        let reg = registry.lock().await;
        reg.auto_connect_servers()
    };

    let count = servers_to_connect.len();

    for (server_id, config) in servers_to_connect {
        // Check if already connected or retry in progress
        {
            let reg = registry.lock().await;
            if reg.is_connected(&server_id) || reg.is_retry_active(&server_id) {
                continue;
            }
        }

        // Clone callback for this server's retry task
        let cb: Option<Box<dyn Fn(&str, &ServerStatus) + Send + Sync>> =
            on_status_change.as_ref().map(|f| {
                let f = Arc::clone(f);
                Box::new(move |id: &str, status: &ServerStatus| f(id, status))
                    as Box<dyn Fn(&str, &ServerStatus) + Send + Sync>
            });

        // Spawn retry task
        let token = spawn_retry_task(Arc::clone(&registry), server_id.clone(), config, cb);

        // Store the token
        {
            let mut reg = registry.lock().await;
            reg.set_retry_token(&server_id, token);
        }
    }

    count
}

/// Convert an MCP Tool to an llm ToolDefinition
fn mcp_tool_to_definition(tool: &Tool) -> ToolDefinition {
    // Convert the MCP JsonObject schema to schemars RootSchema
    let input_schema = serde_json::to_value(&*tool.input_schema)
        .and_then(|v| serde_json::from_value(v))
        .unwrap_or_else(|_| schemars::schema_for!(serde_json::Value));

    ToolDefinition {
        name: tool.name.to_string(),
        description: tool.description.as_ref().map(|d| d.to_string()),
        input_schema,
    }
}

/// Coerce string values to proper types based on the tool's schema.
/// This fixes issues where LLMs (especially local models like Ollama) return
/// string representations of numbers instead of actual numbers.
fn coerce_args_to_schema(
    args: &serde_json::Value,
    schema: &serde_json::Value,
) -> serde_json::Value {
    match (args, schema.get("type").and_then(|t| t.as_str())) {
        // If schema expects an integer and we have a string, parse as integer
        (serde_json::Value::String(s), Some("integer")) => {
            if let Ok(n) = s.parse::<i64>() {
                serde_json::Value::Number(serde_json::Number::from(n))
            } else {
                args.clone()
            }
        }
        // If schema expects a number (float) and we have a string, parse as float
        (serde_json::Value::String(s), Some("number")) => {
            if let Ok(n) = s.parse::<f64>() {
                serde_json::Value::Number(
                    serde_json::Number::from_f64(n).unwrap_or_else(|| serde_json::Number::from(0)),
                )
            } else {
                args.clone()
            }
        }
        // If we have a float but schema expects integer, convert to integer
        (serde_json::Value::Number(n), Some("integer")) => {
            if let Some(f) = n.as_f64() {
                serde_json::Value::Number(serde_json::Number::from(f as i64))
            } else {
                args.clone()
            }
        }
        // If schema expects a boolean and we have a string, try to parse it
        (serde_json::Value::String(s), Some("boolean")) => {
            match s.to_lowercase().as_str() {
                "true" | "1" | "yes" => serde_json::Value::Bool(true),
                "false" | "0" | "no" => serde_json::Value::Bool(false),
                _ => args.clone(),
            }
        }
        // If schema expects an array and we have a string, try to parse as JSON
        (serde_json::Value::String(s), Some("array")) => {
            serde_json::from_str(s).unwrap_or_else(|_| args.clone())
        }
        // If schema expects an object and we have a string, try to parse as JSON
        (serde_json::Value::String(s), Some("object")) => {
            serde_json::from_str(s).unwrap_or_else(|_| args.clone())
        }
        // Recursively handle objects
        (serde_json::Value::Object(obj), _) => {
            let properties = schema.get("properties");
            let mut new_obj = serde_json::Map::new();
            for (key, value) in obj {
                let prop_schema = properties
                    .and_then(|p| p.get(key))
                    .unwrap_or(&serde_json::Value::Null);
                new_obj.insert(key.clone(), coerce_args_to_schema(value, prop_schema));
            }
            serde_json::Value::Object(new_obj)
        }
        // Recursively handle arrays
        (serde_json::Value::Array(arr), _) => {
            let items_schema = schema.get("items").unwrap_or(&serde_json::Value::Null);
            serde_json::Value::Array(
                arr.iter()
                    .map(|item| coerce_args_to_schema(item, items_schema))
                    .collect(),
            )
        }
        // Pass through unchanged
        _ => args.clone(),
    }
}

/// Convert MCP content to our ToolResultContent format
fn mcp_content_to_tool_result(content: &RawContent) -> Option<ToolResultContent> {
    match content {
        RawContent::Text(text) => Some(ToolResultContent::text(&text.text)),
        RawContent::Image(img) => Some(ToolResultContent::image(&img.data, &img.mime_type)),
        RawContent::Audio(audio) => Some(ToolResultContent::audio(&audio.data, &audio.mime_type)),
        RawContent::Resource(resource) => {
            // Extract text from embedded resources
            match &resource.resource {
                rmcp::model::ResourceContents::TextResourceContents { text, .. } => {
                    Some(ToolResultContent::text(text))
                }
                rmcp::model::ResourceContents::BlobResourceContents {
                    blob, mime_type, ..
                } => {
                    // Try to determine if it's image or audio from mime type
                    let mime = mime_type.as_deref().unwrap_or("application/octet-stream");
                    if mime.starts_with("image/") {
                        Some(ToolResultContent::image(blob, mime))
                    } else if mime.starts_with("audio/") {
                        Some(ToolResultContent::audio(blob, mime))
                    } else {
                        // Unknown blob type, skip
                        None
                    }
                }
            }
        }
        RawContent::ResourceLink(_) => {
            // Resource links are references, not content - skip
            None
        }
    }
}

/// A dynamic tool registry that wraps McpRegistry and queries it on each call.
///
/// Unlike static tool registries, this struct dynamically reflects
/// any changes to connected MCP servers - new connections are immediately available.
pub struct McpToolRegistry {
    mcp_registry: Arc<Mutex<McpRegistry>>,
}

impl McpToolRegistry {
    /// Create a new dynamic MCP tool registry
    pub fn new(mcp_registry: Arc<Mutex<McpRegistry>>) -> Self {
        Self { mcp_registry }
    }

    /// Get all tool definitions from all connected MCP servers.
    /// This is called fresh each time to reflect current connections.
    pub async fn get_all_definitions(&self) -> Vec<ToolDefinition> {
        let registry = self.mcp_registry.lock().await;
        let mut definitions = Vec::new();

        for (_server_id, server) in registry.connected_servers() {
            for tool in &server.tools {
                definitions.push(mcp_tool_to_definition(tool));
            }
        }

        definitions
    }

    /// Call a tool by name, routing to the appropriate MCP server.
    /// Returns multimodal content (text, images, audio).
    pub async fn call(&self, name: &str, args: serde_json::Value) -> Result<Vec<ToolResultContent>> {
        traffic_log::log_mcp_request(name, &args);

        let registry = self.mcp_registry.lock().await;

        // Find which server has this tool
        for (_server_id, server) in registry.connected_servers() {
            if let Some(tool) = server.tools.iter().find(|t| t.name == name) {
                // Coerce arguments to match the tool's schema
                // This fixes issues where LLMs return strings for numeric values
                let schema = serde_json::to_value(&*tool.input_schema).unwrap_or_default();
                let coerced_args = coerce_args_to_schema(&args, &schema);
                let arguments = coerced_args.as_object().cloned();

                match server.call_tool(name.to_string(), arguments).await {
                    Ok(result) => {
                        // Convert MCP content to our ToolResultContent format
                        let content: Vec<ToolResultContent> = result
                            .content
                            .into_iter()
                            .filter_map(|c| mcp_content_to_tool_result(&c.raw))
                            .collect();

                        traffic_log::log_mcp_response(name, &content);
                        return Ok(content);
                    }
                    Err(e) => {
                        traffic_log::log_mcp_error(name, &e.to_string());
                        return Err(e);
                    }
                }
            }
        }

        let err_msg = format!("Tool '{}' not found in any connected MCP server", name);
        traffic_log::log_mcp_error(name, &err_msg);
        Err(anyhow::anyhow!(err_msg))
    }

    /// Check if a tool exists in any connected server
    pub async fn has_tool(&self, name: &str) -> bool {
        let registry = self.mcp_registry.lock().await;
        for (_server_id, server) in registry.connected_servers() {
            if server.tools.iter().any(|t| t.name == name) {
                return true;
            }
        }
        false
    }
}
