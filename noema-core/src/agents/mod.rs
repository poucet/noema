//! Concrete agent implementations

mod execution_context;
pub mod mcp_agent;

pub use execution_context::ExecutionContext;
pub use mcp_agent::{McpAgent, ToolEnricher};
