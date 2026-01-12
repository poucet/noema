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
//! - `ConversationManagement`: High-level conversation CRUD (list, delete, rename)

use anyhow::Result;
use async_trait::async_trait;

// Types
pub mod types;

// Trait definition
pub mod turn_store;

// SQLite implementation
#[cfg(feature = "sqlite")]
pub(crate) mod sqlite;

// Re-export types
pub use types::{
    ConversationInfo, MessageContentInfo, MessageInfo, MessageRole, MessageWithContent,
    SpanInfo, SpanRole, SpanWithMessages,
    TurnInfo, TurnWithContent, ViewInfo, ViewSelection,
};

// Re-export trait
pub use turn_store::TurnStore;

// ============================================================================
// ConversationManagement Trait
// ============================================================================

/// Trait for conversation CRUD operations
///
/// This trait provides high-level operations for managing conversations as a whole
/// (listing, deleting, renaming) - separate from `TurnStore` which handles the
/// internal Turn/Span/Message/View structure.
#[async_trait]
pub trait ConversationManagement: Send + Sync {
    /// List all conversations for a user
    async fn list_conversations(&self, user_id: &str) -> Result<Vec<ConversationInfo>>;

    /// Delete a conversation and all its data
    async fn delete_conversation(&self, conversation_id: &str) -> Result<()>;

    /// Rename a conversation
    async fn rename_conversation(&self, conversation_id: &str, name: Option<&str>) -> Result<()>;

    /// Get privacy setting for a conversation
    async fn get_conversation_private(&self, conversation_id: &str) -> Result<bool>;

    /// Set privacy setting for a conversation
    async fn set_conversation_private(&self, conversation_id: &str, is_private: bool) -> Result<()>;
}
