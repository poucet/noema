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
pub mod engine;
pub mod manager;
pub mod mcp;
pub mod storage;
pub mod traffic_log;

pub use agent::Agent;
pub use agents::{McpAgent, SimpleAgent, ToolAgent};
pub use context::{ConversationContext, MessagesGuard};

// Shared types
pub use engine::{CommitMode, ToolConfig};

// Legacy engine exports (deprecated, use ConversationManager instead)
pub use engine::{ChatEngine, EngineCommand, EngineEvent};

// New manager API
pub use manager::{ConversationManager, ManagerCommand, ManagerEvent};

pub use mcp::{AuthMethod, McpConfig, McpRegistry, McpToolRegistry, ServerConfig};