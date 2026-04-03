//! Lumina MCP server — exposes Discord actions as tools via MCP.
//!
//! Lumina registers this server with the daemon on connect, making
//! Discord tools available to any daemon agent (Noema, spawned agents, etc).

use rmcp::{
    handler::server::ServerHandler,
    model::*,
    service::{RequestContext, RoleServer},
    ErrorData as McpError,
};
use std::sync::Arc;

/// MCP server exposing Discord tools through Lumina.
#[derive(Clone)]
pub struct LuminaMcpServer {
    inner: Arc<LuminaMcpServerInner>,
}

struct LuminaMcpServerInner {
    // Will hold serenity Http + cache for Discord API calls in Stage 3.2
}

impl LuminaMcpServer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(LuminaMcpServerInner {}),
        }
    }

    fn get_tools() -> Vec<Tool> {
        // Discord tools will be added in Stage 3.2
        vec![]
    }
}

impl ServerHandler for LuminaMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "lumina-discord".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "Lumina Discord MCP server. Provides tools for interacting with Discord \
                 channels, messages, guilds, and users."
                    .into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            let name = request.name.as_ref();
            tracing::info!("lumina mcp: tool call: {}", name);

            // Tools will be routed here in Stage 3.2
            Ok(CallToolResult::error(vec![Content::text(format!(
                "Unknown tool: {}",
                name
            ))]))
        }
    }
}
