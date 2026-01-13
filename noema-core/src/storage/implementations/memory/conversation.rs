//! In-memory ConversationStore implementation
//!
//! Handles conversation lifecycle (create, list, delete, rename, privacy).
//! TurnStore is separate - use MemoryTurnStore for turn/span/message/view operations.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::storage::ids::{ConversationId, UserId, ViewId};
use crate::storage::traits::ConversationStore;
use crate::storage::types::ConversationInfo;

use super::turn::MemoryTurnStore;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Storage entry with user_id (not in ConversationInfo) and optional main_view_id
/// (main_view_id is required in ConversationInfo but set after creation)
#[derive(Clone, Debug)]
struct ConversationEntry {
    user_id: UserId,
    main_view_id: Option<ViewId>,
    info: ConversationInfoPartial,
}

/// Partial info (everything except main_view_id which is stored separately)
#[derive(Clone, Debug)]
struct ConversationInfoPartial {
    id: ConversationId,
    name: Option<String>,
    turn_count: usize,
    is_private: bool,
    created_at: i64,
    updated_at: i64,
}

impl ConversationEntry {
    fn to_info(&self) -> Option<ConversationInfo> {
        Some(ConversationInfo {
            id: self.info.id.clone(),
            name: self.info.name.clone(),
            main_view_id: self.main_view_id.clone()?,
            turn_count: self.info.turn_count,
            is_private: self.info.is_private,
            created_at: self.info.created_at,
            updated_at: self.info.updated_at,
        })
    }
}

/// In-memory conversation store for testing
///
/// Wraps MemoryTurnStore and adds conversation lifecycle methods.
#[derive(Debug)]
pub struct MemoryConversationStore {
    conversations: Mutex<HashMap<String, ConversationEntry>>,
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
        let entry = ConversationEntry {
            user_id: user_id.clone(),
            main_view_id: None, // Coordinator sets this after creating view
            info: ConversationInfoPartial {
                id: id.clone(),
                name: name.map(|s| s.to_string()),
                turn_count: 0,
                is_private: false,
                created_at: now,
                updated_at: now,
            },
        };
        self.conversations
            .lock()
            .unwrap()
            .insert(id.as_str().to_string(), entry);
        id
    }

    /// Increment turn count for a conversation (for testing)
    pub fn increment_turn_count(&self, conversation_id: &ConversationId) {
        if let Some(entry) = self
            .conversations
            .lock()
            .unwrap()
            .get_mut(conversation_id.as_str())
        {
            entry.info.turn_count += 1;
            entry.info.updated_at = now();
        }
    }

    /// Get the inner turn store for direct TurnStore access
    pub fn turn_store(&self) -> &Arc<MemoryTurnStore> {
        &self.turn_store
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
        Ok(conversations
            .get(conversation_id.as_str())
            .and_then(|e| e.to_info()))
    }

    async fn list_conversations(&self, user_id: &UserId) -> Result<Vec<ConversationInfo>> {
        let conversations = self.conversations.lock().unwrap();
        let mut result: Vec<_> = conversations
            .values()
            .filter(|e| e.user_id == *user_id)
            .filter_map(|e| e.to_info())
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
        if let Some(entry) = self
            .conversations
            .lock()
            .unwrap()
            .get_mut(conversation_id.as_str())
        {
            entry.info.name = name.map(|s| s.to_string());
            entry.info.updated_at = now();
        }
        Ok(())
    }

    async fn is_conversation_private(&self, conversation_id: &ConversationId) -> Result<bool> {
        let conversations = self.conversations.lock().unwrap();
        Ok(conversations
            .get(conversation_id.as_str())
            .map(|e| e.info.is_private)
            .unwrap_or(false))
    }

    async fn set_conversation_private(
        &self,
        conversation_id: &ConversationId,
        is_private: bool,
    ) -> Result<()> {
        if let Some(entry) = self
            .conversations
            .lock()
            .unwrap()
            .get_mut(conversation_id.as_str())
        {
            entry.info.is_private = is_private;
            entry.info.updated_at = now();
        }
        Ok(())
    }

    async fn set_main_view_id(
        &self,
        conversation_id: &ConversationId,
        view_id: &ViewId,
    ) -> Result<()> {
        if let Some(entry) = self
            .conversations
            .lock()
            .unwrap()
            .get_mut(conversation_id.as_str())
        {
            entry.main_view_id = Some(view_id.clone());
            entry.info.updated_at = now();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::traits::TurnStore;

    #[tokio::test]
    async fn test_list_conversations() {
        let store = MemoryConversationStore::new();
        let user_id = UserId::new();

        // Create some conversations and set their main views
        let conv1 = store.create_conversation_sync(&user_id, Some("First"));
        let view1 = store.turn_store.create_view().await.unwrap();
        store.set_main_view_id(&conv1, &view1.id).await.unwrap();

        let conv2 = store.create_conversation_sync(&user_id, Some("Second"));
        let view2 = store.turn_store.create_view().await.unwrap();
        store.set_main_view_id(&conv2, &view2.id).await.unwrap();

        let convs = store.list_conversations(&user_id).await.unwrap();
        assert_eq!(convs.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_conversation() {
        let store = MemoryConversationStore::new();
        let user_id = UserId::new();

        let conv_id = store.create_conversation_sync(&user_id, Some("Test"));
        let view = store.turn_store.create_view().await.unwrap();
        store.set_main_view_id(&conv_id, &view.id).await.unwrap();

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
        let view = store.turn_store.create_view().await.unwrap();
        store.set_main_view_id(&conv_id, &view.id).await.unwrap();

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
