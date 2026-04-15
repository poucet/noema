//! Persistent conversation entity management.

use async_trait::async_trait;
use simply_rpc::RequestContext;
use super::types::{ConversationId, ConversationInfo, ResolvedMessage};

/// CRUD on stored conversations.
///
/// Conversations exist in storage independently of sessions. A session
/// is a runtime handle to an open conversation.
#[simply_rpc::rpc_service("conversation")]
#[async_trait]
pub trait ConversationApi: Send + Sync {
    /// Create a new conversation. Returns the conversation ID.
    #[rpc(post = "/conversation")]
    async fn create_conversation(&self, ctx: &RequestContext, name: Option<String>) -> anyhow::Result<ConversationId>;

    /// List all conversations for the current user.
    #[rpc(get = "/conversation")]
    async fn list_conversations(&self, ctx: &RequestContext) -> anyhow::Result<Vec<ConversationInfo>>;

    /// Delete a conversation (closes session if open, deletes entity).
    #[rpc(delete = "/conversation/{conversation_id}")]
    async fn delete_conversation(&self, ctx: &RequestContext, conversation_id: &ConversationId) -> anyhow::Result<()>;

    /// Rename a conversation.
    #[rpc(put = "/conversation/{conversation_id}")]
    async fn rename_conversation(&self, ctx: &RequestContext, conversation_id: &ConversationId, name: &str) -> anyhow::Result<()>;

    /// Get messages for a conversation (resolved content with turn IDs).
    #[rpc(get = "/conversation/{conversation_id}/messages")]
    async fn get_messages(&self, ctx: &RequestContext, conversation_id: &ConversationId) -> anyhow::Result<Vec<ResolvedMessage>>;
}
