//! SQLite-backed session storage
//!
//! Persistent storage using SQLite. Messages are serialized as JSON.

use super::traits::{SessionStore, StorageTransaction};
use crate::ConversationContext;
use anyhow::{Context, Result};
use async_trait::async_trait;
use llm::{api::Role, ChatMessage, ChatPayload};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Shared SQLite connection pool
///
/// This is the main entry point for SQLite storage. Create one store
/// and use it to create multiple sessions (conversations).
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Open or create a SQLite database at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Create an in-memory SQLite database (useful for testing)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                payload JSON NOT NULL,
                position INTEGER NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                FOREIGN KEY (conversation_id) REFERENCES conversations(id)
            );

            CREATE INDEX IF NOT EXISTS idx_messages_conversation
                ON messages(conversation_id, position);
            "#,
        )
        .context("Failed to initialize database schema")?;
        Ok(())
    }

    /// Create a new conversation session (lazy - not persisted until first message)
    pub fn create_conversation(&self) -> Result<SqliteSession> {
        let id = Uuid::new_v4().to_string();
        // Don't insert into DB yet - will be done on first commit
        Ok(SqliteSession {
            conn: self.conn.clone(),
            conversation_id: id,
            cache: Vec::new(),
            persisted: false,
        })
    }

    /// Open an existing conversation
    pub fn open_conversation(&self, conversation_id: &str) -> Result<SqliteSession> {
        let messages = {
            let conn = self.conn.lock().unwrap();

            // Verify conversation exists
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
                params![conversation_id],
                |row| row.get(0),
            )?;

            if !exists {
                anyhow::bail!("Conversation not found: {}", conversation_id);
            }

            // Load messages
            let mut stmt = conn.prepare(
                "SELECT role, payload FROM messages
                 WHERE conversation_id = ?1
                 ORDER BY position",
            )?;

            let messages: Vec<ChatMessage> = stmt
                .query_map(params![conversation_id], |row| {
                    let role_str: String = row.get(0)?;
                    let payload_json: String = row.get(1)?;
                    Ok((role_str, payload_json))
                })?
                .filter_map(|r| r.ok())
                .filter_map(|(role_str, payload_json)| {
                    let role = match role_str.as_str() {
                        "user" => Role::User,
                        "assistant" => Role::Assistant,
                        "system" => Role::System,
                        _ => return None,
                    };
                    let payload: ChatPayload = serde_json::from_str(&payload_json).ok()?;
                    Some(ChatMessage::new(role, payload))
                })
                .collect();

            messages
        };

        Ok(SqliteSession {
            conn: self.conn.clone(),
            conversation_id: conversation_id.to_string(),
            cache: messages,
            persisted: true, // Already exists in DB
        })
    }

    /// List all conversation IDs
    pub fn list_conversations(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id FROM conversations ORDER BY updated_at DESC")?;
        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }

    /// Delete a conversation and all its messages
    pub fn delete_conversation(&self, conversation_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![conversation_id],
        )?;
        conn.execute(
            "DELETE FROM conversations WHERE id = ?1",
            params![conversation_id],
        )?;
        Ok(())
    }
}

/// SQLite-backed transaction
pub struct SqliteTransaction {
    conversation_id: String,
    committed: Vec<ChatMessage>,
    pending: Vec<ChatMessage>,
    finalized: bool,
}

impl SqliteTransaction {
    fn new(conversation_id: String, committed: Vec<ChatMessage>) -> Self {
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

/// SQLite-backed session for a single conversation
pub struct SqliteSession {
    conn: Arc<Mutex<Connection>>,
    conversation_id: String,
    /// In-memory cache of messages (kept in sync with DB)
    cache: Vec<ChatMessage>,
    /// Whether this conversation has been persisted to the database
    persisted: bool,
}

impl SqliteSession {
    /// Get the conversation ID
    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    fn write_messages(&self, messages: &[ChatMessage], start_position: usize) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        for (i, msg) in messages.iter().enumerate() {
            let id = Uuid::new_v4().to_string();
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };
            let payload_json = serde_json::to_string(&msg.payload)?;
            let position = (start_position + i) as i64;

            conn.execute(
                "INSERT INTO messages (id, conversation_id, role, payload, position)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![&id, &self.conversation_id, role, &payload_json, position],
            )?;
        }

        // Update conversation timestamp
        conn.execute(
            "UPDATE conversations SET updated_at = strftime('%s', 'now') WHERE id = ?1",
            params![&self.conversation_id],
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
        SqliteTransaction::new(self.conversation_id.clone(), self.cache.clone())
    }

    async fn commit(&mut self, transaction: Self::Transaction) -> Result<()> {
        let start_position = self.cache.len();
        let messages = transaction.commit();

        if messages.is_empty() {
            return Ok(());
        }

        // Create conversation in DB on first commit (lazy creation)
        if !self.persisted {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO conversations (id) VALUES (?1)",
                params![&self.conversation_id],
            )?;
            drop(conn);
            self.persisted = true;
        }

        // Write messages to database
        self.write_messages(&messages, start_position)?;

        // Update cache
        self.cache.extend(messages);

        Ok(())
    }

    async fn clear(&mut self) -> Result<()> {
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "DELETE FROM messages WHERE conversation_id = ?1",
                params![&self.conversation_id],
            )?;
        }
        self.cache.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_store_create() {
        let store = SqliteStore::in_memory().unwrap();
        let session = store.create_conversation().unwrap();
        assert!(session.is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_session_commit() {
        let store = SqliteStore::in_memory().unwrap();
        let mut session = store.create_conversation().unwrap();

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));
        tx.add(ChatMessage::assistant("Hi there!".into()));

        session.commit(tx).await.unwrap();

        assert_eq!(session.len(), 2);
        assert_eq!(session.messages()[0].get_text(), "Hello");
        assert_eq!(session.messages()[1].get_text(), "Hi there!");
    }

    #[tokio::test]
    async fn test_sqlite_session_persistence() {
        let store = SqliteStore::in_memory().unwrap();
        let conversation_id;

        // Create and populate session
        {
            let mut session = store.create_conversation().unwrap();
            conversation_id = session.conversation_id().to_string();

            let mut tx = session.begin();
            tx.add(ChatMessage::user("Test message".into()));
            session.commit(tx).await.unwrap();
        }

        // Reopen and verify
        let session = store.open_conversation(&conversation_id).unwrap();
        assert_eq!(session.len(), 1);
        assert_eq!(session.messages()[0].get_text(), "Test message");
    }

    #[tokio::test]
    async fn test_sqlite_list_conversations() {
        let store = SqliteStore::in_memory().unwrap();

        // Empty conversations should not be listed (lazy creation)
        store.create_conversation().unwrap();
        store.create_conversation().unwrap();
        let ids = store.list_conversations().unwrap();
        assert_eq!(ids.len(), 0);

        // Conversations with messages should be listed
        let mut session1 = store.create_conversation().unwrap();
        let mut tx = session1.begin();
        tx.add(ChatMessage::user("Hello".into()));
        session1.commit(tx).await.unwrap();

        let mut session2 = store.create_conversation().unwrap();
        let mut tx = session2.begin();
        tx.add(ChatMessage::user("World".into()));
        session2.commit(tx).await.unwrap();

        let ids = store.list_conversations().unwrap();
        assert_eq!(ids.len(), 2);
    }

    #[tokio::test]
    async fn test_sqlite_delete_conversation() {
        let store = SqliteStore::in_memory().unwrap();
        let mut session = store.create_conversation().unwrap();
        let id = session.conversation_id().to_string();

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));
        session.commit(tx).await.unwrap();

        store.delete_conversation(&id).unwrap();

        let ids = store.list_conversations().unwrap();
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_clear() {
        let store = SqliteStore::in_memory().unwrap();
        let mut session = store.create_conversation().unwrap();
        let id = session.conversation_id().to_string();

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));
        session.commit(tx).await.unwrap();
        assert_eq!(session.len(), 1);

        session.clear().await.unwrap();
        assert!(session.is_empty());

        // Verify cleared in DB too
        let reopened = store.open_conversation(&id).unwrap();
        assert!(reopened.is_empty());
    }

    #[test]
    fn test_sqlite_transaction_rollback() {
        let store = SqliteStore::in_memory().unwrap();
        let session = store.create_conversation().unwrap();

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));

        // Rollback instead of commit
        tx.rollback();

        // Session should still be empty
        assert!(session.is_empty());
    }
}
