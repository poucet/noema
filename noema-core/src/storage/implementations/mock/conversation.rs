//! Mock conversation store for testing

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{ConversationId, UserId, ViewId};
use crate::storage::traits::ConversationStore;
use crate::storage::types::{Conversation, Stored};

/// Mock conversation store that returns unimplemented for all operations
pub struct MockConversationStore;

#[async_trait]
impl ConversationStore for MockConversationStore {
    async fn create_conversation(&self, _: &UserId, _: Option<&str>) -> Result<ConversationId> {
        unimplemented!()
    }
    async fn get_conversation(&self, _: &ConversationId) -> Result<Option<Stored<ConversationId, Conversation>>> {
        unimplemented!()
    }
    async fn list_conversations(&self, _: &UserId) -> Result<Vec<Stored<ConversationId, Conversation>>> {
        unimplemented!()
    }
    async fn delete_conversation(&self, _: &ConversationId) -> Result<()> {
        unimplemented!()
    }
    async fn rename_conversation(&self, _: &ConversationId, _: Option<&str>) -> Result<()> {
        unimplemented!()
    }
    async fn is_conversation_private(&self, _: &ConversationId) -> Result<bool> {
        unimplemented!()
    }
    async fn set_conversation_private(&self, _: &ConversationId, _: bool) -> Result<()> {
        unimplemented!()
    }
    async fn set_main_view_id(&self, _: &ConversationId, _: &ViewId) -> Result<()> {
        unimplemented!()
    }
}
