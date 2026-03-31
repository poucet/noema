//! MCP (Model Context Protocol) support for connecting to tool servers

mod config;
mod registry;

pub use config::{AuthMethod, McpConfig, ServerConfig};
pub use registry::{
    spawn_retry_task, start_auto_connect, ConnectedServer, McpRegistry, McpToolRegistry,
    ServerStatus,
};
