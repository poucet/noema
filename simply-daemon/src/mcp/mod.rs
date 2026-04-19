//! MCP — registry management, OAuth, and MCP HTTP server utilities.
//!
//! - `server` — generic MCP HTTP server (used by Lumina to host its tools)
//! - `service` — unified MCP service (registry + OAuth + auto-connect)

pub mod auth;
pub mod config_io;
pub mod server;
pub mod service;

pub use auth::{DaemonMcpConfig, DaemonServerConfig, ServerAuth};
pub use server::{start_server, start_server_on, ServerHandle};
pub use service::{McpService, McpServiceConfig};
