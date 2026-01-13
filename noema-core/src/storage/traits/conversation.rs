//! ConversationStore trait for conversation lifecycle operations
//!
//! This trait handles conversation-level CRUD: create, list, delete, rename, privacy.
//! TurnStore is a separate trait for internal structure (turns, spans, messages, views).

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::ConversationId;
use crate::storage::ids::UserId;
use crate::storage::types::{Conversation, Stored};

/// Trait for conversation lifecycle operations
///
/// Handles conversation metadata and lifecycle:
/// - Creation, listing, deletion
/// - Naming and privacy settings
///
/// TurnStore is used separately for internal structure (turns, spans, messages, views).
#[async_trait]
pub trait ConversationStore: Send + Sync {
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
    ) -> Result<Option<Stored<ConversationId, Conversation>>>;

    /// List all conversations for a user
    async fn list_conversations(&self, user_id: &UserId) -> Result<Vec<Stored<ConversationId, Conversation>>>;

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

    /// Set the main view ID for a conversation
    async fn set_main_view_id(
        &self,
        conversation_id: &ConversationId,
        view_id: &crate::storage::ids::ViewId,
    ) -> Result<()>;
}
