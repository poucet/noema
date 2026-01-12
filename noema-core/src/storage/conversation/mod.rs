//! Conversation storage traits and implementations
//!
//! This module provides two conversation models that coexist during migration:
//!
//! ## Legacy Model (ConversationStore)
//! - Threads, SpanSets, Spans, Messages structure
//! - Used by the existing app for backwards compatibility
//! - Will be removed in Phase 3.9
//!
//! ## New Model (TurnStore)
//! - Turns, Spans, Messages, Views structure
//! - Supports parallel responses, forking, and editing
//! - Used during dual-write migration period

// Types for both models
pub mod types;

// Trait definitions
pub mod conversation_store;
pub mod turn_store;

// SQLite implementation
#[cfg(feature = "sqlite")]
pub(crate) mod sqlite;

// Re-export legacy types and trait
pub use types::{
    LegacyConversationInfo, LegacySpanInfo, LegacySpanSetInfo, LegacySpanSetWithContent,
    LegacySpanType, LegacyThreadInfo,
};
pub use conversation_store::ConversationStore;

// Re-export new types and trait
pub use types::{
    MessageInfo, MessageRole, NewMessage, SpanInfo as NewSpanInfo, SpanRole, SpanWithMessages,
    TurnInfo, TurnWithContent, ViewInfo, ViewSelection,
};
pub use turn_store::TurnStore;
