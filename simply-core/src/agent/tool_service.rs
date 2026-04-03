//! Trait for tool listing and execution.
//!
//! The session manager and agent use this trait to interact with tools.
//! Concrete implementations include `McpToolRegistry` (real tools) and
//! `EmptyToolService` (no tools — used when tools are disabled).

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};

/// Minimal trait for tool listing and execution.
///
/// The agent calls `get_definitions()` each iteration to discover available
/// tools, and `call_tool()` to execute them. Implementations can wrap MCP
/// servers, add approval flows, or return nothing (empty).
#[async_trait]
pub trait ToolService: Send + Sync {
    /// List all currently available tool definitions.
    async fn get_definitions(&self) -> Vec<ToolDefinition>;

    /// Call a tool by name with JSON arguments.
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<Vec<ToolResultContent>>;
}

/// No-op tool service — returns no tools and rejects all calls.
pub struct EmptyToolService;

#[async_trait]
impl ToolService for EmptyToolService {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        vec![]
    }

    async fn call_tool(
        &self,
        name: &str,
        _arguments: serde_json::Value,
    ) -> Result<Vec<ToolResultContent>> {
        Err(anyhow::anyhow!("no tool service configured, cannot call '{name}'"))
    }
}
