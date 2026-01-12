//! SQLite storage backend
//!
//! Provides `SqliteStore` - a shared SQLite connection wrapper that
//! implements all storage traits for conversation, asset, document,
//! content block, and user management.
//!
//! All trait implementations are in submodules:
//! - `asset` - AssetStore impl
//! - `content_block` - ContentBlockStore impl
//! - `conversation` - TurnStore + ConversationStore impl
//! - `document` - DocumentStore impl
//! - `user` - UserStore impl

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

use crate::storage::coordinator::DynStorageCoordinator;

// Submodules with trait implementations
mod asset;
mod content_block;
mod conversation;
mod document;
mod user;

// Re-export init_schema functions for use in SqliteStore::init_schema
pub(crate) use asset::init_schema as init_asset_schema;
pub(crate) use content_block::init_schema as init_content_block_schema;
pub(crate) use conversation::init_schema as init_conversation_schema;
pub(crate) use document::init_schema as init_document_schema;
pub(crate) use user::init_schema as init_user_schema;

// Re-export sync helpers from conversation module
pub use conversation::sync_helpers;
pub(crate) use content_block::store_content_sync;

/// Shared SQLite connection pool
///
/// This is the main entry point for SQLite storage. Create one store
/// and share it via `Arc` across all components that need database access.
///
/// Implements all storage traits:
/// - `TurnStore` - Turn/Span/Message conversation storage
/// - `ConversationStore` - Conversation-level CRUD
/// - `ContentBlockStore` - Content-addressed text storage
/// - `AssetStore` - Asset metadata storage
/// - `DocumentStore` - Document, tab, and revision storage
/// - `UserStore` - User account management
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
    /// Storage coordinator for content externalization (optional)
    coordinator: RwLock<Option<Arc<DynStorageCoordinator>>>,
}

impl SqliteStore {
    /// Open or create a SQLite database at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(&path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            coordinator: RwLock::new(None),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Create an in-memory SQLite database (useful for testing)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            coordinator: RwLock::new(None),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Set the storage coordinator for content externalization
    pub fn set_coordinator(&self, coordinator: Arc<DynStorageCoordinator>) {
        let mut guard = self.coordinator.write().unwrap();
        *guard = Some(coordinator);
    }

    /// Get the storage coordinator (if set)
    pub fn coordinator(&self) -> Option<Arc<DynStorageCoordinator>> {
        let guard = self.coordinator.read().unwrap();
        guard.clone()
    }

    /// Get access to the connection (for trait implementations)
    pub fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        init_user_schema(&conn)?;
        init_conversation_schema(&conn)?;
        init_asset_schema(&conn)?;
        init_document_schema(&conn)?;
        init_content_block_schema(&conn)?;
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
