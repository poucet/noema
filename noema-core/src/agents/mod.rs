//! Concrete agent implementations

pub mod mcp_agent;
pub mod simple_agent;
pub mod spawn_handler;
pub mod tool_agent;

pub use mcp_agent::McpAgent;
pub use simple_agent::SimpleAgent;
pub use spawn_handler::{
    ConversationSpawnHandler, SpawnAgentArgs, SpawnHandler, SpawnResult,
    spawn_agent_tool_definition,
};
pub use tool_agent::ToolAgent;
