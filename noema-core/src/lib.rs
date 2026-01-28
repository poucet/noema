//! Core traits and implementations for the noema agent framework
//!
//! This crate provides:
//! - **Traits**: `ConversationContext`, `Agent`
//! - **Implementations**: `SimpleAgent`, `ToolAgent`, `McpAgent`
//! - **MCP Support**: `McpRegistry`, `McpToolRegistry` for Model Context Protocol
//! - **Manager**: `ConversationManager` for orchestrating conversations
//! - **Storage**: `Session<S: StorageTypes>` for DB-agnostic session management
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
pub mod manager;
pub mod mcp;
pub mod storage;
pub mod traffic_log;

pub use agent::Agent;
pub use agents::{McpAgent};
pub use context::{ConversationContext, MessagesGuard};

// New manager API
pub use manager::{CommitMode, ConversationManager, ManagerCommand, ManagerEvent, SharedEventSender, ToolConfig};

pub use mcp::{AuthMethod, McpConfig, McpRegistry, McpToolRegistry, ServerConfig};