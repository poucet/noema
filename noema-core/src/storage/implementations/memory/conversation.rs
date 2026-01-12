//! In-memory ConversationStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::ids::{ConversationId, UserId};
use crate::storage::traits::ConversationStore;
use crate::storage::types::ConversationInfo;

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
#[derive(Debug, Default)]
pub struct MemoryConversationStore {
    conversations: Mutex<HashMap<String, StoredConversation>>,
}

impl MemoryConversationStore {
    pub fn new() -> Self {
        Self::default()
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
