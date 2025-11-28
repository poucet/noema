//! MCP (Model Context Protocol) support for connecting to tool servers

mod config;
mod registry;

pub use config::{AuthMethod, McpConfig, ServerConfig};
pub use registry::{ConnectedServer, McpRegistry, McpToolRegistry};
