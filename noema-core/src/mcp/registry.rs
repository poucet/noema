use crate::mcp::config::{McpConfig, ServerConfig};
use anyhow::Result;
use llm::{ToolDefinition, ToolResultContent};
use rmcp::{
    model::{CallToolRequestParam, RawContent, Tool},
    service::RunningService,
    transport::streamable_http_client::{
        StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
    },
    ServiceExt,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A connected MCP server with its available tools.
pub struct ConnectedServer {
    pub config: ServerConfig,
    pub tools: Vec<Tool>,
    service: RunningService<rmcp::RoleClient, ()>,
}

impl ConnectedServer {
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

/// Registry managing MCP server connections.
pub struct McpRegistry {
    config: McpConfig,
    connections: HashMap<String, ConnectedServer>,
}

impl McpRegistry {
    /// Create a new registry with the given configuration
    pub fn new(config: McpConfig) -> Self {
        Self {
            config,
            connections: HashMap::new(),
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

    /// Connect to a server configuration
    async fn connect_to_server(config: &ServerConfig) -> Result<ConnectedServer> {
        let transport = if let Some(ref token) = config.auth_token {
            let mut transport_config =
                StreamableHttpClientTransportConfig::with_uri(Arc::from(config.url.as_str()));
            transport_config = transport_config.auth_header(token.clone());
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
        let registry = self.mcp_registry.lock().await;

        // Find which server has this tool
        for (_server_id, server) in registry.connected_servers() {
            if server.tools.iter().any(|t| t.name == name) {
                let arguments = args.as_object().cloned();
                let result = server.call_tool(name.to_string(), arguments).await?;

                // Convert MCP content to our ToolResultContent format
                let content = result
                    .content
                    .into_iter()
                    .filter_map(|c| mcp_content_to_tool_result(&c.raw))
                    .collect();

                return Ok(content);
            }
        }

        Err(anyhow::anyhow!(
            "Tool '{}' not found in any connected MCP server",
            name
        ))
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
