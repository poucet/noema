//! Lumina MCP server — exposes Discord actions as tools via MCP.
//!
//! Lumina registers this server with the daemon on connect, making
//! Discord tools available to any daemon agent (Noema, spawned agents, etc).

mod tools;

use rmcp::{
    handler::server::ServerHandler,
    model::*,
    service::{RequestContext, RoleServer},
    ErrorData as McpError,
};
use serenity::all::Cache;
use serenity::http::Http;
use std::sync::{Arc, OnceLock};

// ---------------------------------------------------------------------------
// Macros
// ---------------------------------------------------------------------------

/// Define an MCP tool with a JSON schema.
///
/// ```ignore
/// tool!("send_message", "Send a message",
///     required: ["channel_id", "content"],
///     {
///         "channel_id": { "type": "integer", "description": "Channel ID" },
///         "content": { "type": "string", "description": "Message text" }
///     }
/// )
/// ```
macro_rules! tool {
    ($name:literal, $desc:expr, required: [$($req:literal),*], $props:tt) => {
        Tool {
            name: $name.into(),
            title: None,
            description: Some($desc.into()),
            input_schema: {
                let v = serde_json::json!({
                    "type": "object",
                    "properties": $props,
                    "required": [$($req),*]
                });
                match v {
                    serde_json::Value::Object(map) => std::sync::Arc::new(map),
                    _ => std::sync::Arc::new(serde_json::Map::new()),
                }
            },
            annotations: None,
            output_schema: None,
            icons: None,
            meta: None,
        }
    };
}

/// Route a tool call to a handler method by name.
///
/// Each handler is `async fn(&self, Map) -> Result<Value>`.
/// Success → JSON text content. Error → error content.
macro_rules! dispatch {
    ($self:expr, $name:expr, $args:expr, { $($tool:literal => $handler:ident),* $(,)? }) => {
        match $name {
            $( $tool => $self.$handler($args).await, )*
            _ => Err(anyhow::anyhow!("Unknown tool: {}", $name)),
        }
    };
}

pub(crate) use {tool, dispatch};

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// MCP server exposing Discord tools through Lumina.
#[derive(Clone)]
pub struct LuminaMcpServer {
    inner: Arc<LuminaMcpServerInner>,
}

struct LuminaMcpServerInner {
    http: Arc<Http>,
    cache: OnceLock<Arc<Cache>>,
}

impl LuminaMcpServer {
    pub fn new(http: Arc<Http>) -> Self {
        Self {
            inner: Arc::new(LuminaMcpServerInner {
                http,
                cache: OnceLock::new(),
            }),
        }
    }

    /// Inject the serenity cache once the gateway is connected.
    /// Called from the `ready` event handler.
    pub fn set_cache(&self, cache: Arc<Cache>) {
        let _ = self.inner.cache.set(cache);
    }

    fn http(&self) -> &Arc<Http> {
        &self.inner.http
    }

    fn cache(&self) -> Option<&Arc<Cache>> {
        self.inner.cache.get()
    }
}

// ---------------------------------------------------------------------------
// ServerHandler
// ---------------------------------------------------------------------------

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
            tools: tools::definitions(),
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
            let args = request.arguments.unwrap_or_default();

            tracing::info!(tool = name, "lumina mcp: tool call");

            let result = dispatch!(self, name, args, {
                "send_message"          => handle_send_message,
                "reply_to_message"      => handle_reply_to_message,
                "create_embed"          => handle_create_embed,
                "create_poll"           => handle_create_poll,
                "get_channel_history"   => handle_get_channel_history,
                "search_messages"       => handle_search_messages,
                "list_channels"         => handle_list_channels,
                "list_guilds"           => handle_list_guilds,
                "get_emoji_list"        => handle_get_emoji_list,
                "get_voice_states"      => handle_get_voice_states,
                "get_message_stats"     => handle_get_message_stats,
                "get_channel_peak_hours"=> handle_get_channel_peak_hours,
                "get_trending_content"  => handle_get_trending_content,
                "get_active_threads"    => handle_get_active_threads,
                "get_user_activity"     => handle_get_user_activity,
            });

            Ok(match result {
                Ok(value) => CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&value).unwrap_or_default(),
                )]),
                Err(e) => {
                    tracing::warn!(tool = name, error = %e, "tool call failed");
                    CallToolResult::error(vec![Content::text(format!("{e}")])
                    )
                }
            })
        }
    }
}
