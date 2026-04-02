//! MCP — daemon tool server, registry management, and OAuth.
//!
//! - `tools` + `server` — daemon's own MCP server exposing tools to sidecars
//! - `service` — unified MCP service (registry + OAuth + auto-connect)

pub mod server;
pub mod service;
pub mod tools;

pub use server::{start_server, start_server_on, ServerHandle};
pub use service::{McpService, McpServiceConfig};
pub use tools::DaemonMcpServer;
