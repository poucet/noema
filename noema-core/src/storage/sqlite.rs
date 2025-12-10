//! SQLite-backed session storage
//!
//! Persistent storage using SQLite with the unified schema.
//! Supports users, threads, messages, and blob asset references.

use super::content::StoredPayload;
use super::traits::{SessionStore, StorageTransaction};
use crate::ConversationContext;
use anyhow::{Context, Result};
use async_trait::async_trait;
use llm::{api::Role, ChatMessage};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Default user email for single-tenant local mode
pub const DEFAULT_USER_EMAIL: &str = "human@noema";

/// Get current unix timestamp
fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Information about a conversation for listing/display
#[derive(Debug, Clone)]
pub struct ConversationInfo {
    pub id: String,
    pub name: Option<String>,
    pub message_count: usize,
    /// Unix timestamp when created
    pub created_at: i64,
    /// Unix timestamp when last updated
    pub updated_at: i64,
}

/// A message with StoredPayload (preserves asset refs)
/// Used for sending to UI where refs should be fetched separately
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub role: Role,
    pub payload: StoredPayload,
}

/// Information about a user
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

/// Asset metadata stored in the database
#[derive(Debug, Clone)]
pub struct AssetInfo {
    pub id: String,
    pub mime_type: String,
    pub original_filename: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub local_path: Option<String>,
}

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
        let conn = Connection::open(&path)?;
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

        // Check if tables already exist - if they do, skip schema creation
        // This allows us to work with existing Episteme databases
        let tables_exist: bool = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='users'")
            .and_then(|mut stmt| stmt.exists([]))
            .unwrap_or(false);

        if tables_exist {
            // Database already exists, don't try to recreate schema
            return Ok(());
        }

        // Unified schema for Noema/Episteme
        // Timestamps are INTEGER (epoch milliseconds)
        conn.execute_batch(
            r#"
            -- Users
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                email TEXT UNIQUE NOT NULL,
                encrypted_anthropic_key TEXT,
                encrypted_openai_key TEXT,
                encrypted_gemini_key TEXT,
                google_oauth_refresh_token TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            -- Conversations
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
                title TEXT,
                system_prompt TEXT,
                summary_text TEXT,
                summary_embedding BLOB,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            -- Threads (for conversation branching)
            CREATE TABLE IF NOT EXISTS threads (
                id TEXT PRIMARY KEY,
                conversation_id TEXT REFERENCES conversations(id) ON DELETE CASCADE,
                parent_message_id TEXT,
                created_at INTEGER NOT NULL
            );

            -- Messages (Episteme-compatible: content, sequence_number, status)
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                thread_id TEXT REFERENCES threads(id) ON DELETE CASCADE,
                role TEXT CHECK(role IN ('user', 'assistant', 'system')),
                content TEXT NOT NULL,
                text_content TEXT,
                embedding BLOB,
                provider TEXT,
                model TEXT,
                tokens_used INTEGER,
                sequence_number INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'completed',
                created_at INTEGER NOT NULL
            );

            -- Assets (CAS metadata)
            CREATE TABLE IF NOT EXISTS assets (
                id TEXT PRIMARY KEY,
                mime_type TEXT NOT NULL,
                original_filename TEXT,
                file_size_bytes INTEGER,
                metadata_json TEXT,
                local_path TEXT,
                created_at INTEGER NOT NULL
            );

            -- Indexes
            CREATE INDEX IF NOT EXISTS idx_conversations_user ON conversations(user_id);
            CREATE INDEX IF NOT EXISTS idx_threads_conversation ON threads(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id, sequence_number);
            "#,
        )
        .context("Failed to initialize database schema")?;
        Ok(())
    }

    /// Get or create the default user for single-tenant mode
    pub fn get_or_create_default_user(&self) -> Result<UserInfo> {
        let conn = self.conn.lock().unwrap();

        // Try to get existing user
        let user: Option<UserInfo> = conn
            .query_row(
                "SELECT id, email FROM users WHERE email = ?1",
                params![DEFAULT_USER_EMAIL],
                |row| {
                    Ok(UserInfo {
                        id: row.get(0)?,
                        email: row.get(1)?,
                    })
                },
            )
            .ok();

        if let Some(u) = user {
            return Ok(u);
        }

        // Create default user
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();
        conn.execute(
            "INSERT INTO users (id, email, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![&id, DEFAULT_USER_EMAIL, now, now],
        )?;

        Ok(UserInfo {
            id,
            email: DEFAULT_USER_EMAIL.to_string(),
        })
    }

    /// Get user by email
    pub fn get_user_by_email(&self, email: &str) -> Result<Option<UserInfo>> {
        let conn = self.conn.lock().unwrap();
        let user = conn
            .query_row(
                "SELECT id, email FROM users WHERE email = ?1",
                params![email],
                |row| {
                    Ok(UserInfo {
                        id: row.get(0)?,
                        email: row.get(1)?,
                    })
                },
            )
            .ok();
        Ok(user)
    }

    /// List all users in the database
    pub fn list_users(&self) -> Result<Vec<UserInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, email FROM users ORDER BY created_at")?;
        let users = stmt
            .query_map([], |row| {
                Ok(UserInfo {
                    id: row.get(0)?,
                    email: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(users)
    }

    /// Register an asset in the database
    pub fn register_asset(
        &self,
        hash: &str,
        mime_type: &str,
        original_filename: Option<&str>,
        file_size_bytes: Option<i64>,
        local_path: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "INSERT OR IGNORE INTO assets (id, mime_type, original_filename, file_size_bytes, local_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![hash, mime_type, original_filename, file_size_bytes, local_path, now],
        )?;
        Ok(())
    }

    /// Get asset info by hash
    pub fn get_asset(&self, hash: &str) -> Result<Option<AssetInfo>> {
        let conn = self.conn.lock().unwrap();
        let asset = conn
            .query_row(
                "SELECT id, mime_type, original_filename, file_size_bytes, local_path FROM assets WHERE id = ?1",
                params![hash],
                |row| {
                    Ok(AssetInfo {
                        id: row.get(0)?,
                        mime_type: row.get(1)?,
                        original_filename: row.get(2)?,
                        file_size_bytes: row.get(3)?,
                        local_path: row.get(4)?,
                    })
                },
            )
            .ok();
        Ok(asset)
    }

    /// Create a new conversation session (lazy - not persisted until first message)
    pub fn create_conversation(&self) -> Result<SqliteSession> {
        self.create_conversation_for_user(None)
    }

    /// Create a new conversation session for a specific user
    pub fn create_conversation_for_user(&self, user_id: Option<&str>) -> Result<SqliteSession> {
        let id = Uuid::new_v4().to_string();
        // Don't insert into DB yet - will be done on first commit
        Ok(SqliteSession {
            conn: self.conn.clone(),
            conversation_id: id,
            user_id: user_id.map(String::from),
            cache: Vec::new(),
            persisted: false,
        })
    }

    /// Open an existing conversation with resolved asset references
    ///
    /// This async method loads the conversation and resolves any asset references
    /// to inline base64 data using the provided resolver function.
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - The conversation to open
    /// * `resolver` - Async function that takes an asset_id and returns binary data
    pub async fn open_conversation<F, Fut, E>(
        &self,
        conversation_id: &str,
        resolver: F,
    ) -> Result<SqliteSession>
    where
        F: Fn(String) -> Fut + Clone,
        Fut: std::future::Future<Output = Result<Vec<u8>, E>>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let (stored_messages, user_id) = {
            let conn = self.conn.lock().unwrap();

            // Verify conversation exists and get user_id
            let conv_info: Option<Option<String>> = conn
                .query_row(
                    "SELECT user_id FROM conversations WHERE id = ?1",
                    params![conversation_id],
                    |row| row.get(0),
                )
                .ok();

            let user_id = match conv_info {
                Some(uid) => uid,
                None => anyhow::bail!("Conversation not found: {}", conversation_id),
            };

            // Load messages from the main thread as StoredMessage (with refs)
            // Note: Episteme uses 'content' and 'sequence_number', Noema uses 'content_json' and 'position'
            let query = "SELECT m.role, m.content FROM messages m
                 JOIN threads t ON m.thread_id = t.id
                 WHERE t.conversation_id = ?1 AND t.parent_message_id IS NULL
                 ORDER BY m.sequence_number";

            let mut stmt = conn.prepare(query)?;

            let messages: Vec<StoredMessage> = stmt
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
                    let payload: StoredPayload = serde_json::from_str(&payload_json).ok()?;
                    Some(StoredMessage { role, payload })
                })
                .collect();

            (messages, user_id)
        };

        // Resolve asset refs to inline base64 data
        let mut messages = Vec::with_capacity(stored_messages.len());
        for msg in stored_messages {
            let mut payload = msg.payload;
            payload
                .resolve(resolver.clone())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to resolve asset: {}", e))?;
            let chat_payload = payload.to_chat_payload()?;
            messages.push(ChatMessage::new(msg.role, chat_payload));
        }

        Ok(SqliteSession {
            conn: self.conn.clone(),
            conversation_id: conversation_id.to_string(),
            user_id,
            cache: messages,
            persisted: true, // Already exists in DB
        })
    }

    /// List conversations for a specific user
    pub fn list_conversations(&self, user_id: &str) -> Result<Vec<ConversationInfo>> {
        let conn = self.conn.lock().unwrap();

        let query = "SELECT c.id, c.title, COUNT(m.id) as msg_count, c.created_at, c.updated_at
             FROM conversations c
             LEFT JOIN threads t ON t.conversation_id = c.id
             LEFT JOIN messages m ON m.thread_id = t.id
             WHERE c.user_id = ?1
             GROUP BY c.id
             ORDER BY c.updated_at DESC";

        let mut stmt = conn.prepare(query)?;
        let infos: Vec<ConversationInfo> = stmt
            .query_map(params![user_id], |row| {
                Ok(ConversationInfo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    message_count: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(infos)
    }

    /// Load messages and resolve asset references for sending to LLM
    ///
    /// This method loads messages from the database and resolves any asset
    /// references to inline base64 data using the provided resolver function.
    /// Use this when preparing messages to send to an LLM provider.
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - The conversation to load
    /// * `resolver` - Async function that takes an asset_id and returns binary data
    pub async fn load_resolved_messages<F, Fut, E>(
        &self,
        conversation_id: &str,
        resolver: F,
    ) -> Result<Vec<ChatMessage>>
    where
        F: Fn(String) -> Fut + Clone,
        Fut: std::future::Future<Output = Result<Vec<u8>, E>>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let stored_messages = self.load_stored_messages(conversation_id)?;

        let mut resolved = Vec::with_capacity(stored_messages.len());
        for msg in stored_messages {
            let mut payload = msg.payload;
            payload
                .resolve(resolver.clone())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to resolve asset: {}", e))?;
            let chat_payload = payload.to_chat_payload()?;
            resolved.push(ChatMessage::new(msg.role, chat_payload));
        }

        Ok(resolved)
    }

    /// Load messages with StoredPayload (preserves asset refs for UI display)
    ///
    /// Unlike `load_resolved_messages`, this returns `StoredMessage` which keeps
    /// asset references intact. The UI can then fetch assets via the
    /// noema-asset:// protocol for proper HTTP caching.
    pub fn load_stored_messages(&self, conversation_id: &str) -> Result<Vec<StoredMessage>> {
        let conn = self.conn.lock().unwrap();

        let query = "SELECT m.role, m.content FROM messages m
             JOIN threads t ON m.thread_id = t.id
             WHERE t.conversation_id = ?1 AND t.parent_message_id IS NULL
             ORDER BY m.sequence_number";

        let mut stmt = conn.prepare(query)?;

        let messages: Vec<StoredMessage> = stmt
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
                let payload: StoredPayload = serde_json::from_str(&payload_json).ok()?;
                Some(StoredMessage { role, payload })
            })
            .collect();

        Ok(messages)
    }

    /// Rename a conversation
    pub fn rename_conversation(&self, conversation_id: &str, name: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now, conversation_id],
        )?;
        Ok(())
    }

    /// Delete a conversation and all its messages
    pub fn delete_conversation(&self, conversation_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Delete messages through threads (CASCADE should handle this, but be explicit)
        conn.execute(
            "DELETE FROM messages WHERE thread_id IN (SELECT id FROM threads WHERE conversation_id = ?1)",
            params![conversation_id],
        )?;
        conn.execute(
            "DELETE FROM threads WHERE conversation_id = ?1",
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
    /// User ID who owns this conversation
    user_id: Option<String>,
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

    /// Get the user ID (if set)
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    fn write_messages(&self, messages: &[ChatMessage], start_position: usize) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = unix_timestamp();

        // Get the main thread
        let thread_id: String = conn.query_row(
            "SELECT id FROM threads WHERE conversation_id = ?1 AND parent_message_id IS NULL",
            params![&self.conversation_id],
            |row| row.get(0),
        )?;

        for (i, msg) in messages.iter().enumerate() {
            let id = Uuid::new_v4().to_string();
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };
            // Convert to StoredPayload for serialization (supports blob refs)
            let stored_payload: StoredPayload = msg.payload.clone().into();
            let content_json = serde_json::to_string(&stored_payload)?;
            let text_content = msg.get_text();
            let sequence_number = (start_position + i) as i64;

            // Episteme uses 'content' and 'sequence_number', also requires 'status' field
            conn.execute(
                "INSERT INTO messages (id, thread_id, role, content, text_content, sequence_number, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'completed', ?7)",
                params![&id, &thread_id, role, &content_json, &text_content, sequence_number, now],
            )?;
        }

        // Update conversation timestamp
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, &self.conversation_id],
        )?;

        Ok(())
    }

    fn create_conversation_record(&self, conn: &Connection) -> Result<()> {
        let now = unix_timestamp();

        // Create conversation
        conn.execute(
            "INSERT INTO conversations (id, user_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![&self.conversation_id, &self.user_id, now, now],
        )?;

        // Create main thread for this conversation
        let thread_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO threads (id, conversation_id, parent_message_id, created_at) VALUES (?1, ?2, NULL, ?3)",
            params![&thread_id, &self.conversation_id, now],
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
            self.create_conversation_record(&conn)?;
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
                "DELETE FROM messages WHERE thread_id IN (SELECT id FROM threads WHERE conversation_id = ?1)",
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

        // Reopen and verify (no assets, so resolver is never called)
        let session = store
            .open_conversation(&conversation_id, |_: String| async {
                Err::<Vec<u8>, std::io::Error>(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no assets in test",
                ))
            })
            .await
            .unwrap();
        assert_eq!(session.len(), 1);
        assert_eq!(session.messages()[0].get_text(), "Test message");
    }

    #[tokio::test]
    async fn test_sqlite_list_conversations() {
        let store = SqliteStore::in_memory().unwrap();
        let user = store.get_or_create_default_user().unwrap();

        // Empty conversations should not be listed (lazy creation)
        store.create_conversation().unwrap();
        store.create_conversation().unwrap();
        let infos = store.list_conversations(&user.id).unwrap();
        assert_eq!(infos.len(), 0);

        // Conversations with messages should be listed
        let mut session1 = store.create_conversation_for_user(Some(&user.id)).unwrap();
        let mut tx = session1.begin();
        tx.add(ChatMessage::user("Hello".into()));
        session1.commit(tx).await.unwrap();

        let mut session2 = store.create_conversation_for_user(Some(&user.id)).unwrap();
        let mut tx = session2.begin();
        tx.add(ChatMessage::user("World".into()));
        session2.commit(tx).await.unwrap();

        let infos = store.list_conversations(&user.id).unwrap();
        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].message_count, 1);
        assert!(infos[0].name.is_none());
    }

    #[tokio::test]
    async fn test_sqlite_rename_conversation() {
        let store = SqliteStore::in_memory().unwrap();
        let user = store.get_or_create_default_user().unwrap();
        let mut session = store.create_conversation_for_user(Some(&user.id)).unwrap();
        let id = session.conversation_id().to_string();

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));
        session.commit(tx).await.unwrap();

        // Rename
        store.rename_conversation(&id, Some("My Chat")).unwrap();

        let infos = store.list_conversations(&user.id).unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].name.as_deref(), Some("My Chat"));

        // Clear name
        store.rename_conversation(&id, None).unwrap();
        let infos = store.list_conversations(&user.id).unwrap();
        assert!(infos[0].name.is_none());
    }

    #[tokio::test]
    async fn test_sqlite_delete_conversation() {
        let store = SqliteStore::in_memory().unwrap();
        let user = store.get_or_create_default_user().unwrap();
        let mut session = store.create_conversation_for_user(Some(&user.id)).unwrap();
        let id = session.conversation_id().to_string();

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));
        session.commit(tx).await.unwrap();

        store.delete_conversation(&id).unwrap();

        let infos = store.list_conversations(&user.id).unwrap();
        assert!(infos.is_empty());
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

        // Verify cleared in DB too (no assets, so resolver is never called)
        let reopened = store
            .open_conversation(&id, |_: String| async {
                Err::<Vec<u8>, std::io::Error>(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no assets in test",
                ))
            })
            .await
            .unwrap();
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
