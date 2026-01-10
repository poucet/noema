//! SQLite session and transaction implementations

use anyhow::Result;
use async_trait::async_trait;
use llm::{api::Role, ChatMessage};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use super::{SessionStore, StorageTransaction};
use crate::storage::content::StoredPayload;
use crate::storage::content_block::sqlite::store_content_sync;
use crate::storage::conversation::SpanType;
use crate::storage::helper::unix_timestamp;
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

    /// Get access to the connection (for trait implementations)
    pub(crate) fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Initialize each domain's schema
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
    conversation_id: String,
    /// User ID who owns this conversation
    user_id: Option<String>,
    /// In-memory cache of messages (kept in sync with DB)
    cache: Vec<ChatMessage>,
    /// Whether this conversation has been persisted to the database
    persisted: bool,
}

impl SqliteSession {
    /// Create a new session (internal use - called from SqliteStore)
    pub(crate) fn new(
        conn: Arc<Mutex<Connection>>,
        conversation_id: String,
        user_id: Option<String>,
        cache: Vec<ChatMessage>,
        persisted: bool,
    ) -> Self {
        Self {
            conn,
            conversation_id,
            user_id,
            cache,
            persisted,
        }
    }

    /// Get the conversation ID
    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    /// Get the user ID (if set)
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    /// Get the main thread ID for this conversation (creates one if it doesn't exist)
    pub fn get_or_create_thread_id(&self) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let now = unix_timestamp();

        match conn.query_row(
            "SELECT id FROM threads WHERE conversation_id = ?1 AND parent_span_id IS NULL",
            params![&self.conversation_id],
            |row| row.get(0),
        ) {
            Ok(id) => Ok(id),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Create main thread for new conversation
                let thread_id = Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO threads (id, conversation_id, parent_span_id, status, created_at) VALUES (?1, ?2, NULL, 'active', ?3)",
                    params![&thread_id, &self.conversation_id, now],
                )?;
                Ok(thread_id)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Write messages as spans to the database
    fn write_as_span(
        &self,
        messages: &[ChatMessage],
        span_type: &str,
        model_id: Option<&str>,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let now = unix_timestamp();

        // Get the main thread, or create one if it doesn't exist
        let thread_id: String = match conn.query_row(
            "SELECT id FROM threads WHERE conversation_id = ?1 AND parent_span_id IS NULL",
            params![&self.conversation_id],
            |row| row.get(0),
        ) {
            Ok(id) => id,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Create main thread for new conversation
                let thread_id = Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO threads (id, conversation_id, parent_span_id, status, created_at) VALUES (?1, ?2, NULL, 'active', ?3)",
                    params![&thread_id, &self.conversation_id, now],
                )?;
                thread_id
            }
            Err(e) => return Err(e.into()),
        };

        // Get next sequence number for span_set
        let sequence_number: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM span_sets WHERE thread_id = ?1",
                params![&thread_id],
                |row| row.get(0),
            )
            .unwrap_or(1);

        // Create span_set
        let span_set_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO span_sets (id, thread_id, sequence_number, span_type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&span_set_id, &thread_id, sequence_number, span_type, now],
        )?;

        // Create span
        let span_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO spans (id, span_set_id, model_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![&span_id, &span_set_id, model_id, now],
        )?;

        // Set this span as the selected one
        conn.execute(
            "UPDATE span_sets SET selected_span_id = ?1 WHERE id = ?2",
            params![&span_id, &span_set_id],
        )?;

        // Write span_messages
        for (i, msg) in messages.iter().enumerate() {
            let msg_id = Uuid::new_v4().to_string();
            let role = msg.role.to_string();
            let stored_payload: StoredPayload = msg.payload.clone().into();
            let content_json = serde_json::to_string(&stored_payload)?;

            // Store text in content_blocks and get content_id
            let text = msg.get_text();
            let content_id = if !text.is_empty() {
                let origin_kind = match msg.role {
                    Role::User => Some("user"),
                    Role::Assistant => Some("assistant"),
                    Role::System => Some("system"),
                    _ => None,
                };
                match store_content_sync(&conn, &text, origin_kind, self.user_id.as_deref(), model_id) {
                    Ok(id) => Some(id),
                    Err(e) => {
                        tracing::warn!("Failed to store content block: {}", e);
                        None
                    }
                }
            } else {
                None
            };

            conn.execute(
                "INSERT INTO span_messages (id, span_id, sequence_number, role, content, content_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![&msg_id, &span_id, i as i64, role, &content_json, &content_id, now],
            )?;
        }

        // Update conversation timestamp
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, &self.conversation_id],
        )?;

        Ok(span_id)
    }

    /// Write multiple model responses as alternates in a single span_set
    pub fn write_parallel_responses(
        &mut self,
        responses: &[(String, Vec<ChatMessage>)],
        selected_index: usize,
    ) -> Result<(String, Vec<String>)> {
        if responses.is_empty() {
            return Err(anyhow::anyhow!("No responses to write"));
        }

        // Ensure conversation exists
        if !self.persisted {
            let conn = self.conn.lock().unwrap();
            self.create_conversation_record(&conn)?;
            drop(conn);
            self.persisted = true;
        }

        let conn = self.conn.lock().unwrap();
        let now = unix_timestamp();

        // Get the main thread
        let thread_id: String = match conn.query_row(
            "SELECT id FROM threads WHERE conversation_id = ?1 AND parent_span_id IS NULL",
            params![&self.conversation_id],
            |row| row.get(0),
        ) {
            Ok(id) => id,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let thread_id = Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO threads (id, conversation_id, parent_span_id, status, created_at) VALUES (?1, ?2, NULL, 'active', ?3)",
                    params![&thread_id, &self.conversation_id, now],
                )?;
                thread_id
            }
            Err(e) => return Err(e.into()),
        };

        // Get next sequence number
        let sequence_number: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM span_sets WHERE thread_id = ?1",
                params![&thread_id],
                |row| row.get(0),
            )
            .unwrap_or(1);

        // Create a single span_set for all alternates
        let span_set_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO span_sets (id, thread_id, sequence_number, span_type, created_at) VALUES (?1, ?2, ?3, 'assistant', ?4)",
            params![&span_set_id, &thread_id, sequence_number, now],
        )?;

        let mut span_ids = Vec::new();
        let mut selected_span_id = None;

        // Create a span for each model's response
        for (idx, (model_id, messages)) in responses.iter().enumerate() {
            let span_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO spans (id, span_set_id, model_id, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![&span_id, &span_set_id, model_id, now],
            )?;

            // Write messages for this span
            for (msg_idx, msg) in messages.iter().enumerate() {
                let msg_id = Uuid::new_v4().to_string();
                let role = msg.role.to_string();
                let stored_payload: StoredPayload = msg.payload.clone().into();
                let content_json = serde_json::to_string(&stored_payload)?;

                // Store text in content_blocks and get content_id
                let text = msg.get_text();
                let content_id = if !text.is_empty() {
                    let origin_kind = match msg.role {
                        Role::User => Some("user"),
                        Role::Assistant => Some("assistant"),
                        Role::System => Some("system"),
                        _ => None,
                    };
                    match store_content_sync(&conn, &text, origin_kind, self.user_id.as_deref(), Some(model_id.as_str())) {
                        Ok(id) => Some(id),
                        Err(e) => {
                            tracing::warn!("Failed to store content block: {}", e);
                            None
                        }
                    }
                } else {
                    None
                };

                conn.execute(
                    "INSERT INTO span_messages (id, span_id, sequence_number, role, content, content_id, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![&msg_id, &span_id, msg_idx as i64, role, &content_json, &content_id, now],
                )?;
            }

            if idx == selected_index {
                selected_span_id = Some(span_id.clone());
            }
            span_ids.push(span_id);
        }

        // Set the selected span
        if let Some(sel_id) = &selected_span_id {
            conn.execute(
                "UPDATE span_sets SET selected_span_id = ?1 WHERE id = ?2",
                params![sel_id, &span_set_id],
            )?;
        }

        // Update conversation timestamp
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, &self.conversation_id],
        )?;

        drop(conn);

        // Update cache with the selected response messages
        if selected_index < responses.len() {
            self.cache.extend(responses[selected_index].1.clone());
        }

        Ok((span_set_id, span_ids))
    }

    fn create_conversation_record(&self, conn: &Connection) -> Result<()> {
        let now = unix_timestamp();

        // Create conversation
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&self.conversation_id, &self.user_id, "New Conversation", now, now],
        )?;

        // Create main thread for this conversation (parent_span_id is NULL for main thread)
        let thread_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO threads (id, conversation_id, parent_span_id, status, created_at) VALUES (?1, ?2, NULL, 'active', ?3)",
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

        // Determine span type based on first message role
        let span_type = match messages.first().map(|m| &m.role) {
            Some(Role::User) => SpanType::User,
            Some(Role::Assistant) | Some(Role::System) => SpanType::Assistant,
            None => return Ok(()),
        };

        // Write as span to database
        self.write_as_span(&messages, &span_type.to_string(), None)?;

        // Update cache
        self.cache.extend(messages);

        Ok(())
    }

    async fn clear(&mut self) -> Result<()> {
        {
            let conn = self.conn.lock().unwrap();
            // Delete span_messages via cascade from span_sets
            conn.execute(
                "DELETE FROM span_sets WHERE thread_id IN (SELECT id FROM threads WHERE conversation_id = ?1)",
                params![&self.conversation_id],
            )?;
        }
        self.cache.clear();
        Ok(())
    }

    /// Override to properly save parallel responses as separate spans
    async fn commit_parallel_responses(
        &mut self,
        responses: &[(String, Vec<ChatMessage>)],
        selected_index: usize,
    ) -> Result<(String, Vec<String>)> {
        if responses.is_empty() {
            return Ok((String::new(), Vec::new()));
        }
        let (span_set_id, span_ids) = self.write_parallel_responses(responses, selected_index)?;
        Ok((span_set_id, span_ids))
    }
}

// ============================================================================
// SqliteStore session creation methods
// ============================================================================

impl SqliteStore {
    /// Create a new conversation session for a user (lazy - not persisted until first message)
    pub fn create_conversation(&self, user_id: &str) -> Result<SqliteSession> {
        let id = Uuid::new_v4().to_string();
        // Don't insert into DB yet - will be done on first commit
        Ok(SqliteSession::new(
            self.conn.clone(),
            id,
            Some(user_id.to_string()),
            Vec::new(),
            false,
        ))
    }

    /// Open an existing conversation with resolved asset references
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
        use crate::storage::content::StoredMessage;

        // First, get thread info synchronously
        let (thread_id, parent_span_id, user_id) = {
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

            // Find the main thread for this conversation
            let main_thread: Option<(String, Option<String>)> = conn
                .query_row(
                    "SELECT id, parent_span_id FROM threads WHERE conversation_id = ?1 ORDER BY parent_span_id IS NOT NULL, created_at ASC LIMIT 1",
                    params![conversation_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();

            let (thread_id, parent_span_id) = match main_thread {
                Some(t) => t,
                None => anyhow::bail!("No thread found for conversation: {}", conversation_id),
            };

            (thread_id, parent_span_id, user_id)
        };

        // Now load messages (conn is dropped, so we can await)
        let stored_messages: Vec<StoredMessage> = if parent_span_id.is_some() {
            // For forked conversations, use ConversationStore trait method
            use crate::storage::conversation::ConversationStore;
            self.get_thread_messages_with_ancestry(&thread_id).await?
        } else {
            // Load messages synchronously for non-forked conversations
            let conn = self.conn.lock().unwrap();
            let query = "SELECT sm.role, sm.content
                 FROM span_messages sm
                 JOIN spans s ON sm.span_id = s.id
                 JOIN span_sets ss ON s.span_set_id = ss.id
                 JOIN threads t ON ss.thread_id = t.id
                 WHERE t.conversation_id = ?1 AND t.parent_span_id IS NULL
                   AND s.id = ss.selected_span_id
                 ORDER BY ss.sequence_number, sm.sequence_number";

            let mut stmt = conn.prepare(query)?;

            // Collect results before stmt goes out of scope
            let rows: Vec<(String, String)> = stmt
                .query_map(params![conversation_id], |row| {
                    let role_str: String = row.get(0)?;
                    let payload_json: String = row.get(1)?;
                    Ok((role_str, payload_json))
                })?
                .filter_map(|r| r.ok())
                .collect();

            rows.into_iter()
                .filter_map(|(role_str, payload_json)| {
                    let role = role_str.parse::<Role>().ok()?;
                    let payload: StoredPayload = serde_json::from_str(&payload_json).ok()?;
                    Some(StoredMessage { role, payload })
                })
                .collect()
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

        Ok(SqliteSession::new(
            self.conn.clone(),
            conversation_id.to_string(),
            user_id,
            messages,
            true, // Already exists in DB
        ))
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

    #[tokio::test]
    async fn test_sqlite_session_commit() {
        let store = SqliteStore::in_memory().unwrap();
        use crate::storage::user::UserStore;
        let user = store.get_or_create_default_user().await.unwrap();
        let mut session = store.create_conversation(&user.id).unwrap();

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));
        tx.add(ChatMessage::assistant("Hi there!".into()));

        session.commit(tx).await.unwrap();

        assert_eq!(session.messages().len(), 2);
        assert_eq!(session.messages()[0].get_text(), "Hello");
        assert_eq!(session.messages()[1].get_text(), "Hi there!");
    }

    #[tokio::test]
    async fn test_sqlite_session_persistence() {
        let store = SqliteStore::in_memory().unwrap();
        use crate::storage::user::UserStore;
        let user = store.get_or_create_default_user().await.unwrap();
        let conversation_id;

        // Create and populate session
        {
            let mut session = store.create_conversation(&user.id).unwrap();
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
        assert_eq!(session.messages().len(), 1);
        assert_eq!(session.messages()[0].get_text(), "Test message");
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
