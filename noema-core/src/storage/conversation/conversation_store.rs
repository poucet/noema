//! ConversationStore trait for high-level conversation CRUD operations

use anyhow::Result;
use async_trait::async_trait;

use super::types::ConversationInfo;
use crate::storage::ids::{ConversationId, UserId};

/// Trait for conversation CRUD operations
///
/// This trait provides high-level operations for managing conversations as a whole
/// (listing, deleting, renaming) - separate from `TurnStore` which handles the
/// internal Turn/Span/Message/View structure.
#[async_trait]
pub trait ConversationStore: Send + Sync {
    /// List all conversations for a user
    async fn list_conversations(&self, user_id: &UserId) -> Result<Vec<ConversationInfo>>;

    /// Delete a conversation and all its data
    async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()>;

    /// Rename a conversation
    async fn rename_conversation(&self, conversation_id: &ConversationId, name: Option<&str>) -> Result<()>;

    /// Get privacy setting for a conversation
    async fn is_conversation_private(&self, conversation_id: &ConversationId) -> Result<bool>;

    /// Set privacy setting for a conversation
    async fn set_conversation_private(&self, conversation_id: &ConversationId, is_private: bool) -> Result<()>;
}
