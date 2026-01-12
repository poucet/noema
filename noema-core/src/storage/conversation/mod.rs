//! Conversation storage traits and implementations
//!
//! This module provides the Turn/Span/Message conversation model:
//!
//! - **Turns**: Positions in the conversation sequence (user or assistant)
//! - **Spans**: Alternative responses at a turn (parallel models, regenerations)
//! - **Messages**: Individual content within a span (text, tool calls, etc.)
//! - **Views**: Named paths through spans (main view, forks)
//!
//! Two traits are provided:
//! - `TurnStore`: Low-level operations on turns, spans, messages, and views
//! - `ConversationStore`: High-level conversation CRUD (list, delete, rename)

// Types
pub mod types;

// Trait definitions
pub mod turn_store;
pub mod conversation_store;

// SQLite implementation
#[cfg(feature = "sqlite")]
pub(crate) mod sqlite;

// Re-export types
pub use types::{
    ConversationInfo, MessageContentInfo, MessageInfo, MessageRole, MessageWithContent,
    SpanInfo, SpanRole, SpanWithMessages,
    TurnInfo, TurnWithContent, ViewInfo, ViewSelection,
};

// Re-export traits
pub use turn_store::TurnStore;
pub use conversation_store::ConversationStore;
