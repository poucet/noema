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

/// Document source type (matches episteme)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSource {
    GoogleDrive,
    AiGenerated,
    UserCreated,
}

impl DocumentSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocumentSource::GoogleDrive => "google_drive",
            DocumentSource::AiGenerated => "ai_generated",
            DocumentSource::UserCreated => "user_created",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "google_drive" => Some(DocumentSource::GoogleDrive),
            "ai_generated" => Some(DocumentSource::AiGenerated),
            "user_created" => Some(DocumentSource::UserCreated),
            _ => None,
        }
    }
}

/// Document metadata (episteme-compatible)
#[derive(Debug, Clone)]
pub struct DocumentInfo {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub source: DocumentSource,
    pub source_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Document tab (episteme-compatible)
#[derive(Debug, Clone)]
pub struct DocumentTabInfo {
    pub id: String,
    pub document_id: String,
    pub parent_tab_id: Option<String>,
    pub tab_index: i32,
    pub title: String,
    pub icon: Option<String>,
    pub content_markdown: Option<String>,
    pub referenced_assets: Vec<String>,
    pub source_tab_id: Option<String>,
    pub current_revision_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Document revision (episteme-compatible)
#[derive(Debug, Clone)]
pub struct DocumentRevisionInfo {
    pub id: String,
    pub tab_id: String,
    pub revision_number: i32,
    pub parent_revision_id: Option<String>,
    pub content_markdown: String,
    pub content_hash: String,
    pub referenced_assets: Vec<String>,
    pub created_at: i64,
    pub created_by: String,
}

/// Span type (user input or assistant response)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanType {
    User,
    Assistant,
}

impl SpanType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpanType::User => "user",
            SpanType::Assistant => "assistant",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "user" => Some(SpanType::User),
            "assistant" => Some(SpanType::Assistant),
            _ => None,
        }
    }
}

/// Information about a span (one model's response within a SpanSet)
#[derive(Debug, Clone)]
pub struct SpanInfo {
    pub id: String,
    pub model_id: Option<String>,
    pub message_count: usize,
    pub is_selected: bool,
    pub created_at: i64,
}

/// A SpanSet with its selected span's messages
#[derive(Debug, Clone)]
pub struct SpanSetWithContent {
    pub id: String,
    pub span_type: SpanType,
    pub messages: Vec<StoredMessage>,
    pub alternates: Vec<SpanInfo>,
}

/// Information about a SpanSet (position in conversation)
#[derive(Debug, Clone)]
pub struct SpanSetInfo {
    pub id: String,
    pub thread_id: String,
    pub sequence_number: i64,
    pub span_type: SpanType,
    pub selected_span_id: Option<String>,
    pub created_at: i64,
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

            -- Documents (Episteme-compatible)
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
                title TEXT NOT NULL,
                source TEXT NOT NULL,
                source_id TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            -- Document Tabs (hierarchical structure within documents)
            CREATE TABLE IF NOT EXISTS document_tabs (
                id TEXT PRIMARY KEY,
                document_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
                parent_tab_id TEXT REFERENCES document_tabs(id) ON DELETE CASCADE,
                tab_index INTEGER NOT NULL,
                title TEXT NOT NULL,
                icon TEXT,
                content_markdown TEXT,
                referenced_assets TEXT,
                source_tab_id TEXT,
                current_revision_id TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            -- Document Revisions (version history for tabs)
            CREATE TABLE IF NOT EXISTS document_revisions (
                id TEXT PRIMARY KEY,
                tab_id TEXT REFERENCES document_tabs(id) ON DELETE CASCADE,
                revision_number INTEGER NOT NULL,
                parent_revision_id TEXT REFERENCES document_revisions(id) ON DELETE SET NULL,
                content_markdown TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                referenced_assets TEXT,
                created_at INTEGER NOT NULL,
                created_by TEXT NOT NULL DEFAULT 'import'
            );

            -- Add foreign key for current_revision_id after document_revisions exists
            -- (SQLite doesn't support ALTER TABLE ADD CONSTRAINT, so we handle this at app level)

            -- Indexes
            CREATE INDEX IF NOT EXISTS idx_conversations_user ON conversations(user_id);
            CREATE INDEX IF NOT EXISTS idx_threads_conversation ON threads(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id, sequence_number);
            CREATE INDEX IF NOT EXISTS idx_documents_user ON documents(user_id);
            CREATE INDEX IF NOT EXISTS idx_documents_source ON documents(source);
            CREATE INDEX IF NOT EXISTS idx_documents_user_source_id ON documents(user_id, source, source_id);
            CREATE INDEX IF NOT EXISTS idx_document_tabs_document ON document_tabs(document_id);
            CREATE INDEX IF NOT EXISTS idx_document_tabs_parent ON document_tabs(parent_tab_id);
            CREATE INDEX IF NOT EXISTS idx_document_revisions_tab ON document_revisions(tab_id);

            -- SpanSets: positions in conversation (for parallel model responses)
            CREATE TABLE IF NOT EXISTS span_sets (
                id TEXT PRIMARY KEY,
                thread_id TEXT REFERENCES threads(id) ON DELETE CASCADE,
                sequence_number INTEGER NOT NULL,
                span_type TEXT CHECK(span_type IN ('user', 'assistant')) NOT NULL,
                selected_span_id TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_span_sets_thread ON span_sets(thread_id, sequence_number);

            -- Spans: alternative responses within a SpanSet
            CREATE TABLE IF NOT EXISTS spans (
                id TEXT PRIMARY KEY,
                span_set_id TEXT REFERENCES span_sets(id) ON DELETE CASCADE,
                model_id TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_spans_span_set ON spans(span_set_id);

            -- Span messages: individual messages within a span (for multi-turn agentic responses)
            CREATE TABLE IF NOT EXISTS span_messages (
                id TEXT PRIMARY KEY,
                span_id TEXT REFERENCES spans(id) ON DELETE CASCADE,
                sequence_number INTEGER NOT NULL,
                role TEXT CHECK(role IN ('user', 'assistant', 'system', 'tool')) NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_span_messages_span ON span_messages(span_id, sequence_number);
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

    // ========== Document Methods (Episteme-compatible) ==========

    /// Create a new document
    pub fn create_document(
        &self,
        user_id: &str,
        title: &str,
        source: DocumentSource,
        source_id: Option<&str>,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO documents (id, user_id, title, source, source_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![&id, user_id, title, source.as_str(), source_id, now, now],
        )?;

        Ok(id)
    }

    /// Get a document by ID
    pub fn get_document(&self, id: &str) -> Result<Option<DocumentInfo>> {
        let conn = self.conn.lock().unwrap();
        let doc = conn
            .query_row(
                "SELECT id, user_id, title, source, source_id, created_at, updated_at
                 FROM documents WHERE id = ?1",
                params![id],
                |row| {
                    let source_str: String = row.get(3)?;
                    Ok(DocumentInfo {
                        id: row.get(0)?,
                        user_id: row.get(1)?,
                        title: row.get(2)?,
                        source: DocumentSource::from_str(&source_str).unwrap_or(DocumentSource::UserCreated),
                        source_id: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .ok();
        Ok(doc)
    }

    /// Get a document by source and source_id (e.g., find by Google Doc ID)
    pub fn get_document_by_source(&self, user_id: &str, source: DocumentSource, source_id: &str) -> Result<Option<DocumentInfo>> {
        let conn = self.conn.lock().unwrap();
        let doc = conn
            .query_row(
                "SELECT id, user_id, title, source, source_id, created_at, updated_at
                 FROM documents WHERE user_id = ?1 AND source = ?2 AND source_id = ?3",
                params![user_id, source.as_str(), source_id],
                |row| {
                    let source_str: String = row.get(3)?;
                    Ok(DocumentInfo {
                        id: row.get(0)?,
                        user_id: row.get(1)?,
                        title: row.get(2)?,
                        source: DocumentSource::from_str(&source_str).unwrap_or(DocumentSource::UserCreated),
                        source_id: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .ok();
        Ok(doc)
    }

    /// List all documents for a user
    pub fn list_documents(&self, user_id: &str) -> Result<Vec<DocumentInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, title, source, source_id, created_at, updated_at
             FROM documents WHERE user_id = ?1 ORDER BY updated_at DESC",
        )?;

        let docs = stmt
            .query_map(params![user_id], |row| {
                let source_str: String = row.get(3)?;
                Ok(DocumentInfo {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    title: row.get(2)?,
                    source: DocumentSource::from_str(&source_str).unwrap_or(DocumentSource::UserCreated),
                    source_id: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(docs)
    }

    /// Update document title
    pub fn update_document_title(&self, id: &str, title: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE documents SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, id],
        )?;
        Ok(())
    }

    /// Delete a document and all its tabs/revisions
    pub fn delete_document(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        // Revisions and tabs will be cascade deleted
        let rows = conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    // ========== Document Tab Methods ==========

    /// Create a new document tab
    pub fn create_document_tab(
        &self,
        document_id: &str,
        parent_tab_id: Option<&str>,
        tab_index: i32,
        title: &str,
        icon: Option<&str>,
        content_markdown: Option<&str>,
        referenced_assets: &[String],
        source_tab_id: Option<&str>,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();
        let assets_json = serde_json::to_string(referenced_assets)?;

        conn.execute(
            "INSERT INTO document_tabs (id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, referenced_assets, source_tab_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![&id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, &assets_json, source_tab_id, now, now],
        )?;

        Ok(id)
    }

    /// Get a document tab by ID
    pub fn get_document_tab(&self, id: &str) -> Result<Option<DocumentTabInfo>> {
        let conn = self.conn.lock().unwrap();
        let tab = conn
            .query_row(
                "SELECT id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, referenced_assets, source_tab_id, current_revision_id, created_at, updated_at
                 FROM document_tabs WHERE id = ?1",
                params![id],
                |row| {
                    let assets_json: Option<String> = row.get(7)?;
                    let referenced_assets: Vec<String> = assets_json
                        .map(|j| serde_json::from_str(&j).unwrap_or_default())
                        .unwrap_or_default();
                    Ok(DocumentTabInfo {
                        id: row.get(0)?,
                        document_id: row.get(1)?,
                        parent_tab_id: row.get(2)?,
                        tab_index: row.get(3)?,
                        title: row.get(4)?,
                        icon: row.get(5)?,
                        content_markdown: row.get(6)?,
                        referenced_assets,
                        source_tab_id: row.get(8)?,
                        current_revision_id: row.get(9)?,
                        created_at: row.get(10)?,
                        updated_at: row.get(11)?,
                    })
                },
            )
            .ok();
        Ok(tab)
    }

    /// List all tabs for a document
    pub fn list_document_tabs(&self, document_id: &str) -> Result<Vec<DocumentTabInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, referenced_assets, source_tab_id, current_revision_id, created_at, updated_at
             FROM document_tabs WHERE document_id = ?1 ORDER BY tab_index",
        )?;

        let tabs = stmt
            .query_map(params![document_id], |row| {
                let assets_json: Option<String> = row.get(7)?;
                let referenced_assets: Vec<String> = assets_json
                    .map(|j| serde_json::from_str(&j).unwrap_or_default())
                    .unwrap_or_default();
                Ok(DocumentTabInfo {
                    id: row.get(0)?,
                    document_id: row.get(1)?,
                    parent_tab_id: row.get(2)?,
                    tab_index: row.get(3)?,
                    title: row.get(4)?,
                    icon: row.get(5)?,
                    content_markdown: row.get(6)?,
                    referenced_assets,
                    source_tab_id: row.get(8)?,
                    current_revision_id: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tabs)
    }

    /// Update tab content
    pub fn update_document_tab_content(
        &self,
        id: &str,
        content_markdown: &str,
        referenced_assets: &[String],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = unix_timestamp();
        let assets_json = serde_json::to_string(referenced_assets)?;

        conn.execute(
            "UPDATE document_tabs SET content_markdown = ?1, referenced_assets = ?2, updated_at = ?3 WHERE id = ?4",
            params![content_markdown, &assets_json, now, id],
        )?;

        Ok(())
    }

    /// Set current revision for a tab
    pub fn set_document_tab_revision(&self, tab_id: &str, revision_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE document_tabs SET current_revision_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![revision_id, now, tab_id],
        )?;
        Ok(())
    }

    /// Delete a document tab
    pub fn delete_document_tab(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute("DELETE FROM document_tabs WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    /// Update the parent tab reference for a tab
    pub fn update_document_tab_parent(&self, id: &str, parent_tab_id: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE document_tabs SET parent_tab_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![parent_tab_id, now, id],
        )?;
        Ok(())
    }

    /// Search documents by title (case-insensitive)
    pub fn search_documents(&self, user_id: &str, query: &str, limit: usize) -> Result<Vec<DocumentInfo>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT id, user_id, title, source, source_id, created_at, updated_at
             FROM documents
             WHERE user_id = ?1 AND title LIKE ?2 COLLATE NOCASE
             ORDER BY updated_at DESC
             LIMIT ?3",
        )?;

        let docs = stmt
            .query_map(params![user_id, &pattern, limit as i64], |row| {
                let source_str: String = row.get(3)?;
                Ok(DocumentInfo {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    title: row.get(2)?,
                    source: DocumentSource::from_str(&source_str).unwrap_or(DocumentSource::UserCreated),
                    source_id: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(docs)
    }

    // ========== Document Revision Methods ==========

    /// Create a new revision for a tab
    pub fn create_document_revision(
        &self,
        tab_id: &str,
        content_markdown: &str,
        content_hash: &str,
        referenced_assets: &[String],
        created_by: &str,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();
        let assets_json = serde_json::to_string(referenced_assets)?;

        // Get next revision number
        let revision_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(revision_number), 0) + 1 FROM document_revisions WHERE tab_id = ?1",
                params![tab_id],
                |row| row.get(0),
            )
            .unwrap_or(1);

        // Get current revision as parent
        let parent_revision_id: Option<String> = conn
            .query_row(
                "SELECT current_revision_id FROM document_tabs WHERE id = ?1",
                params![tab_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        conn.execute(
            "INSERT INTO document_revisions (id, tab_id, revision_number, parent_revision_id, content_markdown, content_hash, referenced_assets, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![&id, tab_id, revision_number, &parent_revision_id, content_markdown, content_hash, &assets_json, now, created_by],
        )?;

        Ok(id)
    }

    /// Get a revision by ID
    pub fn get_document_revision(&self, id: &str) -> Result<Option<DocumentRevisionInfo>> {
        let conn = self.conn.lock().unwrap();
        let rev = conn
            .query_row(
                "SELECT id, tab_id, revision_number, parent_revision_id, content_markdown, content_hash, referenced_assets, created_at, created_by
                 FROM document_revisions WHERE id = ?1",
                params![id],
                |row| {
                    let assets_json: Option<String> = row.get(6)?;
                    let referenced_assets: Vec<String> = assets_json
                        .map(|j| serde_json::from_str(&j).unwrap_or_default())
                        .unwrap_or_default();
                    Ok(DocumentRevisionInfo {
                        id: row.get(0)?,
                        tab_id: row.get(1)?,
                        revision_number: row.get(2)?,
                        parent_revision_id: row.get(3)?,
                        content_markdown: row.get(4)?,
                        content_hash: row.get(5)?,
                        referenced_assets,
                        created_at: row.get(7)?,
                        created_by: row.get(8)?,
                    })
                },
            )
            .ok();
        Ok(rev)
    }

    /// List revisions for a tab
    pub fn list_document_revisions(&self, tab_id: &str) -> Result<Vec<DocumentRevisionInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, tab_id, revision_number, parent_revision_id, content_markdown, content_hash, referenced_assets, created_at, created_by
             FROM document_revisions WHERE tab_id = ?1 ORDER BY revision_number DESC",
        )?;

        let revs = stmt
            .query_map(params![tab_id], |row| {
                let assets_json: Option<String> = row.get(6)?;
                let referenced_assets: Vec<String> = assets_json
                    .map(|j| serde_json::from_str(&j).unwrap_or_default())
                    .unwrap_or_default();
                Ok(DocumentRevisionInfo {
                    id: row.get(0)?,
                    tab_id: row.get(1)?,
                    revision_number: row.get(2)?,
                    parent_revision_id: row.get(3)?,
                    content_markdown: row.get(4)?,
                    content_hash: row.get(5)?,
                    referenced_assets,
                    created_at: row.get(7)?,
                    created_by: row.get(8)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(revs)
    }

    // ========== SpanSet Methods (for parallel model responses) ==========

    /// Create a new SpanSet (a position in the conversation)
    pub fn create_span_set(
        &self,
        thread_id: &str,
        span_type: SpanType,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();

        // Get next sequence number for this thread
        let sequence_number: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM span_sets WHERE thread_id = ?1",
                params![thread_id],
                |row| row.get(0),
            )
            .unwrap_or(1);

        conn.execute(
            "INSERT INTO span_sets (id, thread_id, sequence_number, span_type, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&id, thread_id, sequence_number, span_type.as_str(), now],
        )?;

        Ok(id)
    }

    /// Create a new Span within a SpanSet (one model's response)
    pub fn create_span(
        &self,
        span_set_id: &str,
        model_id: Option<&str>,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO spans (id, span_set_id, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![&id, span_set_id, model_id, now],
        )?;

        // If this is the first span in the set, make it the selected one
        conn.execute(
            "UPDATE span_sets SET selected_span_id = ?1
             WHERE id = ?2 AND selected_span_id IS NULL",
            params![&id, span_set_id],
        )?;

        Ok(id)
    }

    /// Add a message to a span (for multi-turn agentic responses)
    pub fn add_span_message(
        &self,
        span_id: &str,
        role: Role,
        content: &StoredPayload,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();

        // Get next sequence number for this span
        let sequence_number: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM span_messages WHERE span_id = ?1",
                params![span_id],
                |row| row.get(0),
            )
            .unwrap_or(1);

        let role_str = match role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        };
        let content_json = serde_json::to_string(content)?;

        conn.execute(
            "INSERT INTO span_messages (id, span_id, sequence_number, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![&id, span_id, sequence_number, role_str, &content_json, now],
        )?;

        Ok(id)
    }

    /// Get all spans for a SpanSet with message counts
    pub fn get_span_set_alternates(&self, span_set_id: &str) -> Result<Vec<SpanInfo>> {
        let conn = self.conn.lock().unwrap();

        // Get selected_span_id for this span_set
        let selected_span_id: Option<String> = conn
            .query_row(
                "SELECT selected_span_id FROM span_sets WHERE id = ?1",
                params![span_set_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        let mut stmt = conn.prepare(
            "SELECT s.id, s.model_id, s.created_at,
                    (SELECT COUNT(*) FROM span_messages WHERE span_id = s.id) as msg_count
             FROM spans s
             WHERE s.span_set_id = ?1
             ORDER BY s.created_at",
        )?;

        let spans = stmt
            .query_map(params![span_set_id], |row| {
                let id: String = row.get(0)?;
                Ok(SpanInfo {
                    id: id.clone(),
                    model_id: row.get(1)?,
                    created_at: row.get(2)?,
                    message_count: row.get(3)?,
                    is_selected: selected_span_id.as_ref() == Some(&id),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(spans)
    }

    /// Set the selected span for a SpanSet
    pub fn set_selected_span(&self, span_set_id: &str, span_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE span_sets SET selected_span_id = ?1 WHERE id = ?2",
            params![span_id, span_set_id],
        )?;
        Ok(())
    }

    /// Get messages for a specific span
    pub fn get_span_messages(&self, span_id: &str) -> Result<Vec<StoredMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT role, content FROM span_messages WHERE span_id = ?1 ORDER BY sequence_number",
        )?;

        let messages = stmt
            .query_map(params![span_id], |row| {
                let role_str: String = row.get(0)?;
                let content_json: String = row.get(1)?;
                Ok((role_str, content_json))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(role_str, content_json)| {
                let role = match role_str.as_str() {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    "system" => Role::System,
                    _ => return None,
                };
                let payload: StoredPayload = serde_json::from_str(&content_json).ok()?;
                Some(StoredMessage { role, payload })
            })
            .collect();

        Ok(messages)
    }

    /// Get a SpanSet with its selected span's content
    pub fn get_span_set_with_content(&self, span_set_id: &str) -> Result<Option<SpanSetWithContent>> {
        let conn = self.conn.lock().unwrap();

        // Get span_set info
        let span_set_info: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT span_type, selected_span_id FROM span_sets WHERE id = ?1",
                params![span_set_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (span_type_str, selected_span_id) = match span_set_info {
            Some(info) => info,
            None => return Ok(None),
        };

        let span_type = SpanType::from_str(&span_type_str).unwrap_or(SpanType::User);
        drop(conn); // Release lock before calling other methods

        // Get alternates
        let alternates = self.get_span_set_alternates(span_set_id)?;

        // Get messages from selected span
        let messages = if let Some(ref span_id) = selected_span_id {
            self.get_span_messages(span_id)?
        } else {
            Vec::new()
        };

        Ok(Some(SpanSetWithContent {
            id: span_set_id.to_string(),
            span_type,
            messages,
            alternates,
        }))
    }

    /// Get all SpanSets for a thread in order
    pub fn get_thread_span_sets(&self, thread_id: &str) -> Result<Vec<SpanSetInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, sequence_number, span_type, selected_span_id, created_at
             FROM span_sets WHERE thread_id = ?1 ORDER BY sequence_number",
        )?;

        let span_sets = stmt
            .query_map(params![thread_id], |row| {
                let span_type_str: String = row.get(3)?;
                Ok(SpanSetInfo {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    sequence_number: row.get(2)?,
                    span_type: SpanType::from_str(&span_type_str).unwrap_or(SpanType::User),
                    selected_span_id: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(span_sets)
    }

    /// Helper: Create a user SpanSet with a single span and message
    pub fn add_user_span_set(
        &self,
        thread_id: &str,
        content: &StoredPayload,
    ) -> Result<String> {
        let span_set_id = self.create_span_set(thread_id, SpanType::User)?;
        let span_id = self.create_span(&span_set_id, None)?;
        self.add_span_message(&span_id, Role::User, content)?;
        Ok(span_set_id)
    }

    /// Helper: Create an assistant SpanSet and return the span_set_id
    /// Caller should then create spans for each model
    pub fn add_assistant_span_set(&self, thread_id: &str) -> Result<String> {
        self.create_span_set(thread_id, SpanType::Assistant)
    }

    /// Helper: Add an assistant span with initial message
    pub fn add_assistant_span(
        &self,
        span_set_id: &str,
        model_id: &str,
        content: &StoredPayload,
    ) -> Result<String> {
        let span_id = self.create_span(span_set_id, Some(model_id))?;
        self.add_span_message(&span_id, Role::Assistant, content)?;
        Ok(span_id)
    }

    /// Create a new conversation session for a user (lazy - not persisted until first message)
    pub fn create_conversation(&self, user_id: &str) -> Result<SqliteSession> {
        let id = Uuid::new_v4().to_string();
        // Don't insert into DB yet - will be done on first commit
        Ok(SqliteSession {
            conn: self.conn.clone(),
            conversation_id: id,
            user_id: Some(user_id.to_string()),
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

        // Create conversation with default title (required for Episteme DB compatibility)
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&self.conversation_id, &self.user_id, "New Conversation", now, now],
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
    use crate::storage::StoredContent;

    #[test]
    fn test_sqlite_store_create() {
        let store = SqliteStore::in_memory().unwrap();
        let user = store.get_or_create_default_user().unwrap();
        let session = store.create_conversation(&user.id).unwrap();
        assert!(session.is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_session_commit() {
        let store = SqliteStore::in_memory().unwrap();
        let user = store.get_or_create_default_user().unwrap();
        let mut session = store.create_conversation(&user.id).unwrap();

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
        let user = store.get_or_create_default_user().unwrap();
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
        assert_eq!(session.len(), 1);
        assert_eq!(session.messages()[0].get_text(), "Test message");
    }

    #[tokio::test]
    async fn test_sqlite_list_conversations() {
        let store = SqliteStore::in_memory().unwrap();
        let user = store.get_or_create_default_user().unwrap();

        // Empty conversations should not be listed (lazy creation)
        store.create_conversation(&user.id).unwrap();
        store.create_conversation(&user.id).unwrap();
        let infos = store.list_conversations(&user.id).unwrap();
        assert_eq!(infos.len(), 0);

        // Conversations with messages should be listed
        let mut session1 = store.create_conversation(&user.id).unwrap();
        let mut tx = session1.begin();
        tx.add(ChatMessage::user("Hello".into()));
        session1.commit(tx).await.unwrap();

        let mut session2 = store.create_conversation(&user.id).unwrap();
        let mut tx = session2.begin();
        tx.add(ChatMessage::user("World".into()));
        session2.commit(tx).await.unwrap();

        let infos = store.list_conversations(&user.id).unwrap();
        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].message_count, 1);
        // Conversations now have a default title of "New Conversation"
        assert!(infos[0].name.is_some());
    }

    #[tokio::test]
    async fn test_sqlite_rename_conversation() {
        let store = SqliteStore::in_memory().unwrap();
        let user = store.get_or_create_default_user().unwrap();
        let mut session = store.create_conversation(&user.id).unwrap();
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
        let mut session = store.create_conversation(&user.id).unwrap();
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
        let user = store.get_or_create_default_user().unwrap();
        let mut session = store.create_conversation(&user.id).unwrap();
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
        let user = store.get_or_create_default_user().unwrap();
        let session = store.create_conversation(&user.id).unwrap();

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));

        // Rollback instead of commit
        tx.rollback();

        // Session should still be empty
        assert!(session.is_empty());
    }

    // ========== SpanSet Tests ==========

    /// Helper to create a text StoredPayload for testing
    fn text_payload(s: &str) -> StoredPayload {
        StoredPayload::new(vec![StoredContent::Text { text: s.to_string() }])
    }

    /// Helper to get text from a StoredPayload for testing
    fn get_payload_text(payload: &StoredPayload) -> Option<&str> {
        payload.content.first().and_then(|c| match c {
            StoredContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
    }

    /// Helper to create a thread for testing
    fn create_test_thread(store: &SqliteStore, user_id: &str) -> String {
        let conn = store.conn.lock().unwrap();
        let now = unix_timestamp();
        let conv_id = Uuid::new_v4().to_string();
        let thread_id = Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&conv_id, user_id, "Test Conv", now, now],
        ).unwrap();

        conn.execute(
            "INSERT INTO threads (id, conversation_id, parent_message_id, created_at) VALUES (?1, ?2, NULL, ?3)",
            params![&thread_id, &conv_id, now],
        ).unwrap();

        thread_id
    }

    #[test]
    fn test_span_set_create() {
        let store = SqliteStore::in_memory().unwrap();
        let user = store.get_or_create_default_user().unwrap();
        let thread_id = create_test_thread(&store, &user.id);

        // Create a user span set
        let span_set_id = store.create_span_set(&thread_id, SpanType::User).unwrap();
        assert!(!span_set_id.is_empty());

        // Verify it exists
        let span_sets = store.get_thread_span_sets(&thread_id).unwrap();
        assert_eq!(span_sets.len(), 1);
        assert_eq!(span_sets[0].span_type, SpanType::User);
        assert_eq!(span_sets[0].sequence_number, 1);
    }

    #[test]
    fn test_span_set_with_multiple_alternates() {
        let store = SqliteStore::in_memory().unwrap();
        let user = store.get_or_create_default_user().unwrap();
        let thread_id = create_test_thread(&store, &user.id);

        // Create a user span set with message
        let user_content = text_payload("Hello, which model is best?");
        let _user_span_set_id = store.add_user_span_set(&thread_id, &user_content).unwrap();

        // Create an assistant span set
        let asst_span_set_id = store.add_assistant_span_set(&thread_id).unwrap();

        // Add multiple model responses (alternates)
        let claude_content = text_payload("I'm Claude, happy to help!");
        let gpt_content = text_payload("I'm GPT-4, at your service!");
        let gemini_content = text_payload("I'm Gemini, let me assist!");

        let _claude_span_id = store.add_assistant_span(&asst_span_set_id, "anthropic/claude-sonnet", &claude_content).unwrap();
        let gpt_span_id = store.add_assistant_span(&asst_span_set_id, "openai/gpt-4o", &gpt_content).unwrap();
        let _gemini_span_id = store.add_assistant_span(&asst_span_set_id, "google/gemini-pro", &gemini_content).unwrap();

        // Verify we have 3 alternates
        let alternates = store.get_span_set_alternates(&asst_span_set_id).unwrap();
        assert_eq!(alternates.len(), 3);

        // First one should be selected by default
        assert!(alternates[0].is_selected);
        assert!(!alternates[1].is_selected);
        assert!(!alternates[2].is_selected);

        // Verify models
        assert_eq!(alternates[0].model_id.as_deref(), Some("anthropic/claude-sonnet"));
        assert_eq!(alternates[1].model_id.as_deref(), Some("openai/gpt-4o"));
        assert_eq!(alternates[2].model_id.as_deref(), Some("google/gemini-pro"));

        // Each should have 1 message
        assert_eq!(alternates[0].message_count, 1);
        assert_eq!(alternates[1].message_count, 1);
        assert_eq!(alternates[2].message_count, 1);

        // Get span set with content - should show Claude's response (first/selected)
        let span_set_content = store.get_span_set_with_content(&asst_span_set_id).unwrap().unwrap();
        assert_eq!(span_set_content.span_type, SpanType::Assistant);
        assert_eq!(span_set_content.messages.len(), 1);
        assert_eq!(get_payload_text(&span_set_content.messages[0].payload), Some("I'm Claude, happy to help!"));
        assert_eq!(span_set_content.alternates.len(), 3);

        // Switch to GPT
        store.set_selected_span(&asst_span_set_id, &gpt_span_id).unwrap();

        // Verify selection changed
        let span_set_content = store.get_span_set_with_content(&asst_span_set_id).unwrap().unwrap();
        assert_eq!(get_payload_text(&span_set_content.messages[0].payload), Some("I'm GPT-4, at your service!"));
    }

    #[test]
    fn test_span_with_multiple_messages() {
        let store = SqliteStore::in_memory().unwrap();
        let user = store.get_or_create_default_user().unwrap();
        let thread_id = create_test_thread(&store, &user.id);

        // Create assistant span set
        let span_set_id = store.create_span_set(&thread_id, SpanType::Assistant).unwrap();
        let span_id = store.create_span(&span_set_id, Some("anthropic/claude-sonnet")).unwrap();

        // Simulate agentic multi-turn: assistant -> tool call -> tool result -> assistant
        let msg1 = text_payload("Let me check that for you.");
        let msg2 = text_payload("[Tool call: search]");  // In reality this would be structured
        let msg3 = text_payload("Based on my search, here's what I found...");

        store.add_span_message(&span_id, Role::Assistant, &msg1).unwrap();
        store.add_span_message(&span_id, Role::System, &msg2).unwrap();  // Tool calls often stored as system
        store.add_span_message(&span_id, Role::Assistant, &msg3).unwrap();

        // Verify all messages are in the span
        let messages = store.get_span_messages(&span_id).unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::Assistant);
        assert_eq!(messages[1].role, Role::System);
        assert_eq!(messages[2].role, Role::Assistant);

        // Alternates should show message count of 3
        let alternates = store.get_span_set_alternates(&span_set_id).unwrap();
        assert_eq!(alternates[0].message_count, 3);
    }
}
