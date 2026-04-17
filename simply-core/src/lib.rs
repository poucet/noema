//! Core traits and implementations for the noema agent framework

pub mod agent;
pub mod embedding;
pub mod mcp;
pub mod session_manager;
pub mod storage;
pub mod traffic_log;

pub use agent::{
    Agent, ConversationContext, EmptyToolService, ExecutionContext, InMemoryContext, ToolAgent,
    MessagesGuard, ToolService,
};

pub use session_manager::{
    Persistence, SessionCommand, SessionEvent, SessionEventSender, SessionManager,
};

pub use mcp::{AuthMethod, McpConfig, McpRegistry, McpToolRegistry, OAuthCredentials, ServerConfig};
