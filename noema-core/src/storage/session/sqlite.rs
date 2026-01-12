//! SQLite storage backend
//!
//! Provides SqliteStore which implements TurnStore and ConversationStore.
//! All conversation data is stored via the Turn/Span/Message structure,
//! with content externalized via StorageCoordinator.

use anyhow::Result;
use async_trait::async_trait;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::storage::conversation::{ConversationInfo, ConversationStore};
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{ConversationId, UserId};

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
        assert!(store.coordinator().is_none());
    }
}
