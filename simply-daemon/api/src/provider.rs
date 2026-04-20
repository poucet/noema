//! Unified tool provider abstraction.
//!
//! A `ToolProvider` is anything that provides tools to the daemon:
//! MCP servers, WS-registered skills, or embedded skills. All speak
//! `rmcp` types and are managed identically by the daemon.

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use simply_rpc::RequestContext;

use crate::skill::OAuthRequirement;

/// What kind of provider — used for listing/filtering in admin APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// External MCP server (connected over HTTP/WS).
    Mcp,
    /// In-process skill (embedded via `Skill` trait).
    Skill,
    /// Client-registered tools over WebSocket reverse-RPC.
    WsClient,
}

/// A provider of tools to the daemon.
///
/// All implementations use `rmcp` types for tool definitions and results.
/// The daemon treats all providers identically — only transport differs.
#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// Unique ID for this provider (e.g., "google-docs", "discord").
    fn id(&self) -> &str;

    /// Human-readable display name.
    fn display_name(&self) -> &str;

    /// What kind of provider this is — used by admin listings to filter.
    fn kind(&self) -> ProviderKind {
        ProviderKind::Mcp
    }

    /// Available tools (listed regardless of user auth state).
    async fn tools(&self) -> Vec<Tool>;

    /// OAuth requirements. The daemon handles auth flows and injects tokens
    /// into `RequestContext.tokens` before calling tools.
    async fn oauth_requirements(&self) -> Vec<OAuthRequirement> {
        vec![]
    }

    /// Call a tool. `ctx` carries user identity + OAuth tokens.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        ctx: &RequestContext,
    ) -> Result<CallToolResult>;

    /// Whether this provider is currently reachable.
    fn is_connected(&self) -> bool {
        true
    }
}
