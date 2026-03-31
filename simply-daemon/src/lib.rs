//! Simply Daemon
//!
//! The daemon is the hub of the Simply platform. It owns storage, session
//! management, MCP tool registry, and agent orchestration.
//!
//! Access is through the [`DaemonApi`] trait, which has two implementations:
//! - **In-process** ([`embedded::EmbeddedDaemon`]) — linked directly into the
//!   host binary (Noema, Lumina, tests). No networking.
//! - **Remote** (future) — calls go over WebSocket to a standalone daemon process.

pub mod api;
pub mod embedded;
pub mod mcp;

// Re-export types that clients need — Noema should only depend on simply-daemon.
pub mod types {
    // IDs
    pub use simply_core::storage::ids::{
        AssetId, ConversationId, DocumentId, EntityId, RevisionId, SpanId, TabId, TurnId, UserId,
    };

    // Storage types used by clients
    pub use simply_core::storage::{
        InputContent, ResolvedContent, ResolvedMessage,
        Entity, EntityType,
        Document, DocumentSource, DocumentTab, StoredEditable,
    };

    // LLM types used by clients
    pub use llm::{ChatMessage, ContentBlock, ModelCapability, ModelInfo, Role, ToolResultContent};
}
