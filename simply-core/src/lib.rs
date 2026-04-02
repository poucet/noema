//! Core traits and implementations for the noema agent framework

pub mod agent;
pub mod agents;
pub mod context;
pub mod in_memory_context;
pub mod manager;
pub mod mcp;
pub mod session_manager;
pub mod storage;
pub mod traffic_log;

// Keep old modules temporarily (light_manager, light_session)
pub mod light_manager;
pub mod light_session;

pub use agent::Agent;
pub use agents::McpAgent;
pub use context::{ConversationContext, MessagesGuard};
pub use in_memory_context::InMemoryContext;

// New unified session manager
pub use session_manager::{
    NoStorage, SessionCommand, SessionEvent, SessionEventSender, SessionManager, StorageHook,
};

// Old manager (still used by Noema persistent sessions)
pub use manager::{
    CommitMode, ConversationManager, ManagerCommand, ManagerEvent, SharedEventSender, ToolConfig,
    run_agent,
};

pub use mcp::{AuthMethod, McpConfig, McpRegistry, McpToolRegistry, ServerConfig};
