//! Persistent conversation entity management.

use async_trait::async_trait;
use super::types::{ConversationId, ConversationInfo};

/// CRUD on stored conversations.
///
/// Conversations exist in storage independently of sessions. A session
/// is a runtime handle to an open conversation.
#[simply_rpc::rpc_service("conversation")]
#[async_trait]
pub trait ConversationApi: Send + Sync {
    /// Create a new conversation. Returns the conversation ID.
    async fn create_conversation(&self, name: Option<String>) -> anyhow::Result<ConversationId>;

    /// List all conversations for the current user.
    async fn list_conversations(&self) -> anyhow::Result<Vec<ConversationInfo>>;

    /// Delete a conversation (closes session if open, deletes entity).
    async fn delete_conversation(&self, conversation_id: &ConversationId) -> anyhow::Result<()>;

    /// Rename a conversation.
    async fn rename_conversation(&self, conversation_id: &ConversationId, name: &str) -> anyhow::Result<()>;
}
