//! SQLite session implementation using TurnStore
//!
//! This module provides SQLite-backed conversation sessions that use the
//! Turn/Span/Message structure exclusively. All conversation data is stored
//! via TurnStore, with content externalized via StorageCoordinator.

use anyhow::Result;
use async_trait::async_trait;
use llm::{api::Role, ChatMessage, ContentBlock};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::{SessionStore, StorageTransaction};
use crate::storage::content::{ContentResolver, StoredContent};
use crate::storage::content_block::OriginKind;
use crate::storage::conversation::{ConversationInfo, ConversationStore, TurnStore};
use crate::storage::conversation::types::{MessageRole, SpanRole};
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{ConversationId, UserId};
use crate::ConversationContext;

// ============================================================================
// SqliteStore - Main entry point
// ============================================================================

/// Shared SQLite connection pool
///
/// This is the main entry point for SQLite storage. Create one store
/// and use it to create multiple sessions (conversations).
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
    /// Storage coordinator for content externalization
    coordinator: std::sync::RwLock<Option<Arc<crate::storage::coordinator::DynStorageCoordinator>>>,
}

impl SqliteStore {
    /// Open or create a SQLite database at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(&path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            coordinator: std::sync::RwLock::new(None),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Create an in-memory SQLite database (useful for testing)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            coordinator: std::sync::RwLock::new(None),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Set the storage coordinator for content externalization
    pub fn set_coordinator(&self, coordinator: Arc<crate::storage::coordinator::DynStorageCoordinator>) {
        let mut guard = self.coordinator.write().unwrap();
        *guard = Some(coordinator);
    }

    /// Get the storage coordinator (if set)
    pub fn coordinator(&self) -> Option<Arc<crate::storage::coordinator::DynStorageCoordinator>> {
        let guard = self.coordinator.read().unwrap();
        guard.clone()
    }

    /// Get access to the connection (for trait implementations)
    pub(crate) fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        crate::storage::user::sqlite::init_schema(&conn)?;
        crate::storage::conversation::sqlite::init_schema(&conn)?;
        crate::storage::asset::sqlite::init_schema(&conn)?;
        crate::storage::document::sqlite::init_schema(&conn)?;
        crate::storage::content_block::sqlite::init_schema(&conn)?;
        Ok(())
    }
}

// ============================================================================
// SqliteTransaction
// ============================================================================

/// SQLite-backed transaction
pub struct SqliteTransaction {
    conversation_id: String,
    committed: Vec<ChatMessage>,
    pending: Vec<ChatMessage>,
    finalized: bool,
}

impl SqliteTransaction {
    pub(crate) fn new(conversation_id: String, committed: Vec<ChatMessage>) -> Self {
        Self {
            conversation_id,
            committed,
            pending: Vec::new(),
            finalized: false,
        }
    }

    /// Get the conversation ID this transaction belongs to
    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }
}

impl ConversationContext for SqliteTransaction {
    fn iter(&self) -> impl Iterator<Item = &ChatMessage> {
        self.committed.iter().chain(self.pending.iter())
    }

    fn len(&self) -> usize {
        self.committed.len() + self.pending.len()
    }

    fn add(&mut self, message: ChatMessage) {
        assert!(!self.finalized, "Cannot add to finalized transaction");
        self.pending.push(message);
    }

    fn extend(&mut self, messages: impl IntoIterator<Item = ChatMessage>) {
        assert!(!self.finalized, "Cannot add to finalized transaction");
        self.pending.extend(messages);
    }
}

impl StorageTransaction for SqliteTransaction {
    fn pending(&self) -> &[ChatMessage] {
        &self.pending
    }

    fn committed(&self) -> &[ChatMessage] {
        &self.committed
    }

    fn is_finalized(&self) -> bool {
        self.finalized
    }

    fn commit(mut self) -> Vec<ChatMessage> {
        self.finalized = true;
        std::mem::take(&mut self.pending)
    }

    fn rollback(mut self) {
        self.finalized = true;
        self.pending.clear();
    }
}

impl Drop for SqliteTransaction {
    fn drop(&mut self) {
        if !self.finalized && !self.pending.is_empty() {
            eprintln!(
                "Warning: SqliteTransaction dropped without commit/rollback ({} messages lost)",
                self.pending.len()
            );
        }
    }
}

// ============================================================================
// SqliteSession
// ============================================================================

/// SQLite-backed session for a single conversation
pub struct SqliteSession {
    conn: Arc<Mutex<Connection>>,
    conversation_id: ConversationId,
    /// User ID who owns this conversation
    user_id: Option<String>,
    /// In-memory cache of messages (kept in sync with DB)
    cache: Vec<ChatMessage>,
    /// Whether this conversation has been persisted to the database
    persisted: bool,
    /// Storage coordinator for content externalization
    coordinator: Option<Arc<crate::storage::coordinator::DynStorageCoordinator>>,
}

impl SqliteSession {
    /// Create a new session (internal use - called from SqliteStore)
    pub(crate) fn new(
        conn: Arc<Mutex<Connection>>,
        conversation_id: ConversationId,
        user_id: Option<String>,
        cache: Vec<ChatMessage>,
        persisted: bool,
        coordinator: Option<Arc<crate::storage::coordinator::DynStorageCoordinator>>,
    ) -> Self {
        Self {
            conn,
            conversation_id,
            user_id,
            cache,
            persisted,
            coordinator,
        }
    }

    /// Get the conversation ID
    pub fn conversation_id(&self) -> &str {
        self.conversation_id.as_str()
    }

    /// Get the user ID (if set)
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    /// Write messages as a turn to the database using TurnStore
    async fn write_turn(
        &self,
        store: &SqliteStore,
        messages: &[ChatMessage],
        model_id: Option<&str>,
    ) -> Result<()> {
        if messages.is_empty() {
            return Ok(());
        }

        // Determine span role from first message
        let span_role = match messages.first().map(|m| &m.role) {
            Some(Role::User) => SpanRole::User,
            _ => SpanRole::Assistant,
        };

        // Create turn
        let turn = store.add_turn(&self.conversation_id, span_role).await?;

        // Create span with model_id if specified
        let span = store.add_span(&turn.id, model_id).await?;

        // Store messages
        for msg in messages {
            let content = self.store_message_content(&msg.payload.content, &msg.role).await?;
            let message_role: MessageRole = msg.role.into();
            store.add_message(&span.id, message_role, &content).await?;
        }

        // Select this span in the main view
        if let Some(main_view) = store.get_main_view(&self.conversation_id).await? {
            store.select_span(&main_view.id, &turn.id, &span.id).await?;
        }

        // Update conversation timestamp
        {
            let conn = self.conn.lock().unwrap();
            let now = unix_timestamp();
            conn.execute(
                "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
                params![now, self.conversation_id.as_str()],
            )?;
        }

        Ok(())
    }

    /// Write multiple model responses as parallel spans at the same turn
    async fn write_parallel_turn(
        &self,
        store: &SqliteStore,
        responses: &[(String, Vec<ChatMessage>)],
        selected_index: usize,
    ) -> Result<(String, Vec<String>)> {
        if responses.is_empty() {
            return Err(anyhow::anyhow!("No responses to write"));
        }

        // Create a single turn for all parallel responses
        let turn = store.add_turn(&self.conversation_id, SpanRole::Assistant).await?;
        let turn_id_str = turn.id.as_str().to_string();
        let mut span_ids = Vec::with_capacity(responses.len());

        for (idx, (model_id, messages)) in responses.iter().enumerate() {
            // Create span for this model
            let span = store.add_span(&turn.id, Some(model_id)).await?;
            span_ids.push(span.id.as_str().to_string());

            // Store messages
            for msg in messages {
                let content = self.store_message_content(&msg.payload.content, &msg.role).await?;
                let message_role: MessageRole = msg.role.into();
                store.add_message(&span.id, message_role, &content).await?;
            }

            // Select this span if it's the selected one
            if idx == selected_index {
                if let Some(main_view) = store.get_main_view(&self.conversation_id).await? {
                    store.select_span(&main_view.id, &turn.id, &span.id).await?;
                }
            }
        }

        // Update conversation timestamp
        {
            let conn = self.conn.lock().unwrap();
            let now = unix_timestamp();
            conn.execute(
                "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
                params![now, self.conversation_id.as_str()],
            )?;
        }

        Ok((turn_id_str, span_ids))
    }

    /// Convert ContentBlocks to StoredContent using coordinator
    async fn store_message_content(
        &self,
        content: &[ContentBlock],
        role: &Role,
    ) -> Result<Vec<StoredContent>> {
        let coordinator = self.coordinator.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No storage coordinator configured"))?;

        let origin = match role {
            Role::User => OriginKind::User,
            Role::Assistant => OriginKind::Assistant,
            Role::System => OriginKind::System,
            Role::Tool => OriginKind::System,
        };

        coordinator.store_content(content.to_vec(), origin).await
    }

    fn create_conversation_record(&self, conn: &Connection) -> Result<()> {
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![self.conversation_id.as_str(), &self.user_id, "New Conversation", now, now],
        )?;

        Ok(())
    }
}

#[async_trait]
impl SessionStore for SqliteSession {
    type Transaction = SqliteTransaction;

    fn messages(&self) -> &[ChatMessage] {
        &self.cache
    }

    fn messages_mut(&mut self) -> &mut Vec<ChatMessage> {
        &mut self.cache
    }

    fn begin(&self) -> Self::Transaction {
        SqliteTransaction::new(self.conversation_id.as_str().to_string(), self.cache.clone())
    }

    async fn commit(&mut self, transaction: Self::Transaction) -> Result<()> {
        let messages = transaction.commit();

        if messages.is_empty() {
            return Ok(());
        }

        // Create conversation in DB on first commit (lazy creation)
        if !self.persisted {
            {
                let conn = self.conn.lock().unwrap();
                self.create_conversation_record(&conn)?;
            }
            self.persisted = true;
        }

        // Create a temporary store to access TurnStore methods
        let store = SqliteStore {
            conn: self.conn.clone(),
            coordinator: std::sync::RwLock::new(self.coordinator.clone()),
        };

        // Ensure main view exists
        if store.get_main_view(&self.conversation_id).await?.is_none() {
            store.create_view(&self.conversation_id, Some("main"), true).await?;
        }

        self.write_turn(&store, &messages, None).await?;

        // Update cache
        self.cache.extend(messages);

        Ok(())
    }

    async fn clear(&mut self) -> Result<()> {
        // Delete all turns (cascades to spans, messages, message_content)
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "DELETE FROM turns WHERE conversation_id = ?1",
                params![self.conversation_id.as_str()],
            )?;
            // Also delete views
            conn.execute(
                "DELETE FROM views WHERE conversation_id = ?1",
                params![self.conversation_id.as_str()],
            )?;
        }
        self.cache.clear();
        Ok(())
    }

    async fn commit_parallel_responses(
        &mut self,
        responses: &[(String, Vec<ChatMessage>)],
        selected_index: usize,
    ) -> Result<(String, Vec<String>)> {
        if responses.is_empty() {
            return Ok((String::new(), Vec::new()));
        }

        // Ensure conversation exists
        if !self.persisted {
            {
                let conn = self.conn.lock().unwrap();
                self.create_conversation_record(&conn)?;
            }
            self.persisted = true;
        }

        let store = SqliteStore {
            conn: self.conn.clone(),
            coordinator: std::sync::RwLock::new(self.coordinator.clone()),
        };

        // Ensure main view exists
        if store.get_main_view(&self.conversation_id).await?.is_none() {
            store.create_view(&self.conversation_id, Some("main"), true).await?;
        }

        let result = self.write_parallel_turn(&store, responses, selected_index).await?;

        // Update cache with selected response
        if selected_index < responses.len() {
            self.cache.extend(responses[selected_index].1.clone());
        }

        Ok(result)
    }
}

// ============================================================================
// SqliteStore session creation methods
// ============================================================================

impl SqliteStore {
    /// Create a new conversation session for a user (lazy - not persisted until first message)
    pub fn create_conversation(&self, user_id: &str) -> Result<SqliteSession> {
        let id = ConversationId::new();
        Ok(SqliteSession::new(
            self.conn.clone(),
            id,
            Some(user_id.to_string()),
            Vec::new(),
            false,
            self.coordinator(),
        ))
    }

    /// Open an existing conversation by loading messages from the main view
    pub async fn open_conversation(&self, conversation_id: &str) -> Result<SqliteSession> {
        let conv_id = ConversationId::from_string(conversation_id);

        // Verify conversation exists and get user_id
        let user_id = {
            let conn = self.conn.lock().unwrap();
            let user_id: Option<String> = conn
                .query_row(
                    "SELECT user_id FROM conversations WHERE id = ?1",
                    params![conversation_id],
                    |row| row.get(0),
                )
                .map_err(|_| anyhow::anyhow!("Conversation not found: {}", conversation_id))?;
            user_id
        };

        // Get the main view
        let main_view = self.get_main_view(&conv_id).await?
            .ok_or_else(|| anyhow::anyhow!("No main view found for conversation"))?;

        // Load the view path (all turns with selected spans and messages)
        let view_path = self.get_view_path(&main_view.id).await?;

        // Get coordinator for resolving content
        let coordinator = self.coordinator()
            .ok_or_else(|| anyhow::anyhow!("No coordinator configured for content resolution"))?;

        // Convert to ChatMessages
        let mut messages = Vec::new();
        for turn in view_path {
            for msg_with_content in turn.messages {
                // Resolve content refs to ContentBlocks
                let mut content_blocks = Vec::new();
                for content_info in msg_with_content.content {
                    let block = content_info.content.resolve(&*coordinator).await?;
                    content_blocks.push(block);
                }

                // Convert MessageRole to llm::Role
                let role = match msg_with_content.message.role {
                    MessageRole::User => Role::User,
                    MessageRole::Assistant => Role::Assistant,
                    MessageRole::System => Role::System,
                    MessageRole::Tool => Role::Tool,
                };

                messages.push(ChatMessage::new(role, llm::ChatPayload::new(content_blocks)));
            }
        }

        Ok(SqliteSession::new(
            self.conn.clone(),
            conv_id,
            user_id,
            messages,
            true, // Already exists in DB
            self.coordinator(),
        ))
    }
}

// ============================================================================
// ConversationStore Implementation
// ============================================================================

#[async_trait]
impl ConversationStore for SqliteStore {
    async fn list_conversations(&self, user_id: &UserId) -> Result<Vec<ConversationInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT c.id, c.title, c.is_private, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM turns t WHERE t.conversation_id = c.id) as turn_count
             FROM conversations c
             WHERE c.user_id = ?1
             ORDER BY c.updated_at DESC"
        )?;

        let conversations = stmt
            .query_map(params![user_id.as_str()], |row| {
                let id: String = row.get(0)?;
                let name: Option<String> = row.get(1)?;
                let is_private: i32 = row.get(2)?;
                let created_at: i64 = row.get(3)?;
                let updated_at: i64 = row.get(4)?;
                let turn_count: usize = row.get(5)?;
                Ok((id, name, is_private, created_at, updated_at, turn_count))
            })?
            .filter_map(|r| r.ok())
            .map(|(id, name, is_private, created_at, updated_at, turn_count)| {
                ConversationInfo {
                    id: ConversationId::from_string(id),
                    name,
                    turn_count,
                    is_private: is_private != 0,
                    created_at,
                    updated_at,
                }
            })
            .collect();

        Ok(conversations)
    }

    async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Cascade delete handles turns, spans, messages, message_content, views, view_selections
        conn.execute(
            "DELETE FROM conversations WHERE id = ?1",
            params![conversation_id.as_str()],
        )?;
        Ok(())
    }

    async fn rename_conversation(&self, conversation_id: &ConversationId, name: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, unix_timestamp(), conversation_id.as_str()],
        )?;
        Ok(())
    }

    async fn is_conversation_private(&self, conversation_id: &ConversationId) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let is_private: i32 = conn.query_row(
            "SELECT is_private FROM conversations WHERE id = ?1",
            params![conversation_id.as_str()],
            |row| row.get(0),
        )?;
        Ok(is_private != 0)
    }

    async fn set_conversation_private(&self, conversation_id: &ConversationId, is_private: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE conversations SET is_private = ?1, updated_at = ?2 WHERE id = ?3",
            params![is_private as i32, unix_timestamp(), conversation_id.as_str()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_store_create() {
        let store = SqliteStore::in_memory().unwrap();
        use crate::storage::user::UserStore;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let user = rt.block_on(store.get_or_create_default_user()).unwrap();
        let session = store.create_conversation(&user.id).unwrap();
        assert!(session.messages().is_empty());
    }

    #[test]
    fn test_sqlite_transaction_rollback() {
        let store = SqliteStore::in_memory().unwrap();
        use crate::storage::user::UserStore;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let user = rt.block_on(store.get_or_create_default_user()).unwrap();
        let session = store.create_conversation(&user.id).unwrap();

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));

        // Rollback instead of commit
        tx.rollback();

        // Session should still be empty
        assert!(session.messages().is_empty());
    }
}
