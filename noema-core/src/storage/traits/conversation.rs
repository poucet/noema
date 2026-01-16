//! ConversationStore trait for conversation lifecycle operations
//!
//! **DEPRECATED**: This trait is deprecated in favor of EntityStore.
//! Conversations are now entities with type "conversation" and main_view_id in metadata.
//! Use EntityStore methods instead:
//! - create_conversation → entity_store.create_entity(EntityType::conversation(), ...)
//! - get_conversation → entity_store.get_entity(...)
//! - list_conversations → entity_store.list_entities(..., Some(&EntityType::conversation()))
//! - delete_conversation → entity_store.delete_entity(...)
//! - rename_conversation → entity_store.update_entity(...) with name field
//! - privacy methods → entity_store.update_entity(...) with is_private field
//!
//! This trait is kept for backward compatibility during migration.

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::ConversationId;
use crate::storage::ids::UserId;
use crate::storage::types::{Conversation, Stored};

/// Stored representation of a Conversation (immutable after creation)
pub type StoredConversation = Stored<ConversationId, Conversation>;

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
    ) -> Result<Option<StoredConversation>>;

    /// List all conversations for a user
    async fn list_conversations(&self, user_id: &UserId) -> Result<Vec<StoredConversation>>;

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
