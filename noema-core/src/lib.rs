//! Core traits and implementations for the noema agent framework
//!
//! This crate provides:
//! - **Traits**: `ConversationContext`, `Agent`, `Transaction`
//! - **Implementations**: `SimpleAgent`, `ToolAgent`, `McpAgent`
//! - **MCP Support**: `McpRegistry`, `McpToolRegistry` for Model Context Protocol
//!
//! # Example
//!
//! ```ignore
//! use noema_core::{Agent, SimpleAgent};
//!
//! let agent = SimpleAgent::new();
//! let messages = agent.execute(&context, &model).await?;
//! ```
pub mod agent;
pub mod agents;
pub mod context;
pub mod mcp;
pub mod session;
pub mod transaction;

pub use agent::Agent;
pub use agents::{McpAgent, SimpleAgent, ToolAgent};
pub use context::ConversationContext;
pub use mcp::{McpConfig, McpRegistry, McpToolRegistry, ServerConfig};
pub use session::{Session, SimpleContext};
pub use transaction::Transaction;
