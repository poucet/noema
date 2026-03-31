//! MCP server — exposes daemon capabilities as MCP tools.
//!
//! Formerly the standalone `noema-mcp-core` crate. Lives in the daemon
//! because it depends on storage, sessions, and agent orchestration.

pub mod server;
pub mod tools;

pub use server::{start_server, start_server_on, ServerHandle};
pub use tools::NoemaCoreServer;
