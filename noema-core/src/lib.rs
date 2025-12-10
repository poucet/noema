//! Core traits and implementations for the noema agent framework
//!
//! This crate provides:
//! - **Traits**: `ConversationContext`, `Agent`, `StorageTransaction`
//! - **Implementations**: `SimpleAgent`, `ToolAgent`, `McpAgent`
//! - **MCP Support**: `McpRegistry`, `McpToolRegistry` for Model Context Protocol
//! - **Engine**: `ChatEngine` for managing chat sessions
//! - **Storage**: `SessionStore` trait with `MemorySession` and `SqliteSession` backends
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
pub mod engine;
pub mod mcp;
pub mod storage;
pub mod traffic_log;

pub use agent::Agent;
pub use agents::{McpAgent, SimpleAgent, ToolAgent};
pub use context::ConversationContext;
pub use engine::{ChatEngine, EngineCommand, EngineEvent};
pub use mcp::{AuthMethod, McpConfig, McpRegistry, McpToolRegistry, ServerConfig};
pub use storage::{MemorySession, MemoryTransaction, SessionStore, StorageTransaction};
#[cfg(feature = "sqlite")]
pub use storage::{ConversationInfo, SqliteSession, SqliteStore};