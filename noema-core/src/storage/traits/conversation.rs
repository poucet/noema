//! ConversationStore trait for conversation lifecycle operations
//!
//! This trait handles conversation-level CRUD: create, list, delete, rename, privacy.
//! It extends TurnStore, which manages the internal structure (turns, spans, messages, views).

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{ConversationId, UserId};
use crate::storage::traits::TurnStore;
use crate::storage::types::conversation::ConversationInfo;

/// Trait for conversation lifecycle operations
///
/// Extends TurnStore to provide both:
/// - Conversation lifecycle: creation, listing, deletion, metadata
/// - Internal structure: turns, spans, messages, views (from TurnStore)
#[async_trait]
pub trait ConversationStore: TurnStore {
    /// Create a new conversation for a user
    ///
    /// Creates the conversation record and a main view. Returns the conversation ID.
    async fn create_conversation(
        &self,
        user_id: &UserId,
        name: Option<&str>,
    ) -> Result<ConversationId>;

    /// Get conversation info by ID
    async fn get_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<ConversationInfo>>;

    /// List all conversations for a user
    async fn list_conversations(&self, user_id: &UserId) -> Result<Vec<ConversationInfo>>;

    /// Delete a conversation and all its data
    async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()>;

    /// Rename a conversation
    async fn rename_conversation(
        &self,
        conversation_id: &ConversationId,
        name: Option<&str>,
    ) -> Result<()>;

    /// Get privacy setting for a conversation
    async fn is_conversation_private(&self, conversation_id: &ConversationId) -> Result<bool>;

    /// Set privacy setting for a conversation
    async fn set_conversation_private(
        &self,
        conversation_id: &ConversationId,
        is_private: bool,
    ) -> Result<()>;
}
