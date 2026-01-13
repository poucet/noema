//! In-memory ConversationStore implementation
//!
//! Since ConversationStore extends TurnStore, this implementation wraps
//! MemoryTurnStore and adds conversation lifecycle methods.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::storage::content::StoredContent;
use crate::storage::ids::{ConversationId, MessageId, SpanId, TurnId, UserId, ViewId};
use crate::storage::traits::{ConversationStore, TurnStore};
use crate::storage::types::{
    ConversationInfo, MessageInfo, MessageRole, MessageWithContent, SpanInfo, SpanRole, TurnInfo,
    TurnWithContent, ViewInfo,
};

use super::turn::MemoryTurnStore;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Stored conversation data
#[derive(Clone, Debug)]
struct StoredConversation {
    id: ConversationId,
    user_id: UserId,
    name: Option<String>,
    is_private: bool,
    turn_count: usize,
    created_at: i64,
    updated_at: i64,
}

/// In-memory conversation store for testing
///
/// Wraps MemoryTurnStore and adds conversation lifecycle methods.
#[derive(Debug)]
pub struct MemoryConversationStore {
    conversations: Mutex<HashMap<String, StoredConversation>>,
    turn_store: Arc<MemoryTurnStore>,
}

impl Default for MemoryConversationStore {
    fn default() -> Self {
        Self {
            conversations: Mutex::new(HashMap::new()),
            turn_store: Arc::new(MemoryTurnStore::new()),
        }
    }
}

impl MemoryConversationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a shared turn store (for testing scenarios where you need access to both)
    pub fn with_turn_store(turn_store: Arc<MemoryTurnStore>) -> Self {
        Self {
            conversations: Mutex::new(HashMap::new()),
            turn_store,
        }
    }

    /// Create a new conversation (internal sync method)
    fn create_conversation_sync(&self, user_id: &UserId, name: Option<&str>) -> ConversationId {
        let id = ConversationId::new();
        let now = now();
        let conv = StoredConversation {
            id: id.clone(),
            user_id: user_id.clone(),
            name: name.map(|s| s.to_string()),
            is_private: false,
            turn_count: 0,
            created_at: now,
            updated_at: now,
        };
        self.conversations
            .lock()
            .unwrap()
            .insert(id.as_str().to_string(), conv);
        id
    }

    /// Increment turn count for a conversation (for testing)
    pub fn increment_turn_count(&self, conversation_id: &ConversationId) {
        if let Some(conv) = self
            .conversations
            .lock()
            .unwrap()
            .get_mut(conversation_id.as_str())
        {
            conv.turn_count += 1;
            conv.updated_at = now();
        }
    }
}

// TurnStore implementation - delegates to inner turn_store
#[async_trait]
impl TurnStore for MemoryConversationStore {
    async fn add_turn(&self, conversation_id: &ConversationId, role: SpanRole) -> Result<TurnInfo> {
        self.turn_store.add_turn(conversation_id, role).await
    }
    async fn get_turns(&self, conversation_id: &ConversationId) -> Result<Vec<TurnInfo>> {
        self.turn_store.get_turns(conversation_id).await
    }
    async fn get_turn(&self, turn_id: &TurnId) -> Result<Option<TurnInfo>> {
        self.turn_store.get_turn(turn_id).await
    }
    async fn add_span(&self, turn_id: &TurnId, model_id: Option<&str>) -> Result<SpanInfo> {
        self.turn_store.add_span(turn_id, model_id).await
    }
    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<SpanInfo>> {
        self.turn_store.get_spans(turn_id).await
    }
    async fn get_span(&self, span_id: &SpanId) -> Result<Option<SpanInfo>> {
        self.turn_store.get_span(span_id).await
    }
    async fn add_message(&self, span_id: &SpanId, role: MessageRole, content: &[StoredContent]) -> Result<MessageInfo> {
        self.turn_store.add_message(span_id, role, content).await
    }
    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageInfo>> {
        self.turn_store.get_messages(span_id).await
    }
    async fn get_messages_with_content(&self, span_id: &SpanId) -> Result<Vec<MessageWithContent>> {
        self.turn_store.get_messages_with_content(span_id).await
    }
    async fn get_message(&self, message_id: &MessageId) -> Result<Option<MessageInfo>> {
        self.turn_store.get_message(message_id).await
    }
    async fn create_view(&self, conversation_id: &ConversationId, name: Option<&str>, is_main: bool) -> Result<ViewInfo> {
        self.turn_store.create_view(conversation_id, name, is_main).await
    }
    async fn get_main_view(&self, conversation_id: &ConversationId) -> Result<Option<ViewInfo>> {
        self.turn_store.get_main_view(conversation_id).await
    }
    async fn get_view(&self, view_id: &ViewId) -> Result<Option<ViewInfo>> {
        self.turn_store.get_view(view_id).await
    }
    async fn get_views(&self, conversation_id: &ConversationId) -> Result<Vec<ViewInfo>> {
        self.turn_store.get_views(conversation_id).await
    }
    async fn select_span(&self, view_id: &ViewId, turn_id: &TurnId, span_id: &SpanId) -> Result<()> {
        self.turn_store.select_span(view_id, turn_id, span_id).await
    }
    async fn get_selected_span(&self, view_id: &ViewId, turn_id: &TurnId) -> Result<Option<SpanId>> {
        self.turn_store.get_selected_span(view_id, turn_id).await
    }
    async fn get_view_path(&self, view_id: &ViewId) -> Result<Vec<TurnWithContent>> {
        self.turn_store.get_view_path(view_id).await
    }
    async fn fork_view(&self, view_id: &ViewId, at_turn_id: &TurnId, name: Option<&str>) -> Result<ViewInfo> {
        self.turn_store.fork_view(view_id, at_turn_id, name).await
    }
    async fn fork_view_with_selections(&self, view_id: &ViewId, at_turn_id: &TurnId, name: Option<&str>, selections: &[(TurnId, SpanId)]) -> Result<ViewInfo> {
        self.turn_store.fork_view_with_selections(view_id, at_turn_id, name, selections).await
    }
    async fn get_view_context_at(&self, view_id: &ViewId, turn_id: &TurnId) -> Result<Vec<TurnWithContent>> {
        self.turn_store.get_view_context_at(view_id, turn_id).await
    }
    async fn edit_turn(&self, view_id: &ViewId, turn_id: &TurnId, messages: Vec<(MessageRole, Vec<StoredContent>)>, model_id: Option<&str>, fork_if_not_tip: bool, fork_name: Option<&str>) -> Result<(SpanInfo, Option<ViewInfo>)> {
        self.turn_store.edit_turn(view_id, turn_id, messages, model_id, fork_if_not_tip, fork_name).await
    }
    async fn add_user_turn(&self, conversation_id: &ConversationId, text: &str) -> Result<(TurnInfo, SpanInfo, MessageInfo)> {
        self.turn_store.add_user_turn(conversation_id, text).await
    }
    async fn add_assistant_turn(&self, conversation_id: &ConversationId, text: &str, model_id: &str) -> Result<(TurnInfo, SpanInfo, MessageInfo)> {
        self.turn_store.add_assistant_turn(conversation_id, text, model_id).await
    }
}

#[async_trait]
impl ConversationStore for MemoryConversationStore {
    async fn create_conversation(
        &self,
        user_id: &UserId,
        name: Option<&str>,
    ) -> Result<ConversationId> {
        Ok(self.create_conversation_sync(user_id, name))
    }

    async fn get_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<ConversationInfo>> {
        let conversations = self.conversations.lock().unwrap();
        Ok(conversations.get(conversation_id.as_str()).map(|c| {
            ConversationInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                turn_count: c.turn_count,
                is_private: c.is_private,
                created_at: c.created_at,
                updated_at: c.updated_at,
            }
        }))
    }

    async fn list_conversations(&self, user_id: &UserId) -> Result<Vec<ConversationInfo>> {
        let conversations = self.conversations.lock().unwrap();
        let mut result: Vec<_> = conversations
            .values()
            .filter(|c| c.user_id == *user_id)
            .map(|c| ConversationInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                turn_count: c.turn_count,
                is_private: c.is_private,
                created_at: c.created_at,
                updated_at: c.updated_at,
            })
            .collect();
        result.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(result)
    }

    async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()> {
        self.conversations
            .lock()
            .unwrap()
            .remove(conversation_id.as_str());
        Ok(())
    }

    async fn rename_conversation(
        &self,
        conversation_id: &ConversationId,
        name: Option<&str>,
    ) -> Result<()> {
        if let Some(conv) = self
            .conversations
            .lock()
            .unwrap()
            .get_mut(conversation_id.as_str())
        {
            conv.name = name.map(|s| s.to_string());
            conv.updated_at = now();
        }
        Ok(())
    }

    async fn is_conversation_private(&self, conversation_id: &ConversationId) -> Result<bool> {
        let conversations = self.conversations.lock().unwrap();
        Ok(conversations
            .get(conversation_id.as_str())
            .map(|c| c.is_private)
            .unwrap_or(false))
    }

    async fn set_conversation_private(
        &self,
        conversation_id: &ConversationId,
        is_private: bool,
    ) -> Result<()> {
        if let Some(conv) = self
            .conversations
            .lock()
            .unwrap()
            .get_mut(conversation_id.as_str())
        {
            conv.is_private = is_private;
            conv.updated_at = now();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_conversations() {
        let store = MemoryConversationStore::new();
        let user_id = UserId::new();

        // Create some conversations
        let _conv1 = store.create_conversation_sync(&user_id, Some("First"));
        let _conv2 = store.create_conversation_sync(&user_id, Some("Second"));

        let convs = store.list_conversations(&user_id).await.unwrap();
        assert_eq!(convs.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_conversation() {
        let store = MemoryConversationStore::new();
        let user_id = UserId::new();

        let conv_id = store.create_conversation_sync(&user_id, Some("Test"));

        let convs = store.list_conversations(&user_id).await.unwrap();
        assert_eq!(convs.len(), 1);

        store.delete_conversation(&conv_id).await.unwrap();

        let convs = store.list_conversations(&user_id).await.unwrap();
        assert_eq!(convs.len(), 0);
    }

    #[tokio::test]
    async fn test_rename_conversation() {
        let store = MemoryConversationStore::new();
        let user_id = UserId::new();

        let conv_id = store.create_conversation_sync(&user_id, Some("Original"));

        let convs = store.list_conversations(&user_id).await.unwrap();
        assert_eq!(convs[0].name, Some("Original".to_string()));

        store
            .rename_conversation(&conv_id, Some("Renamed"))
            .await
            .unwrap();

        let convs = store.list_conversations(&user_id).await.unwrap();
        assert_eq!(convs[0].name, Some("Renamed".to_string()));
    }

    #[tokio::test]
    async fn test_privacy_setting() {
        let store = MemoryConversationStore::new();
        let user_id = UserId::new();

        let conv_id = store.create_conversation_sync(&user_id, Some("Test"));

        assert!(!store.is_conversation_private(&conv_id).await.unwrap());

        store.set_conversation_private(&conv_id, true).await.unwrap();
        assert!(store.is_conversation_private(&conv_id).await.unwrap());

        store
            .set_conversation_private(&conv_id, false)
            .await
            .unwrap();
        assert!(!store.is_conversation_private(&conv_id).await.unwrap());
    }
}
