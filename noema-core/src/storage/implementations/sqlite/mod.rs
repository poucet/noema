//! SQLite storage backend
//!
//! Provides `SqliteStore` - a shared SQLite connection wrapper that
//! implements all storage traits for conversation, asset, document,
//! content block, and user management.
//!
//! All trait implementations are in submodules:
//! - `asset` - AssetStore impl
//! - `content_block` - TextStore impl
//! - `conversation` - ConversationStore impl
//! - `turn` - TurnStore impl
//! - `document` - DocumentStore impl
//! - `user` - UserStore impl

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

// Submodules with trait implementations
mod asset;
mod conversation;
mod document;
mod text;
mod turn;
mod user;

// Re-export init_schema functions for use in SqliteStore::init_schema
pub(crate) use asset::init_schema as init_asset_schema;
pub(crate) use conversation::init_schema as init_conversation_schema;
pub(crate) use document::init_schema as init_document_schema;
pub(crate) use text::init_schema as init_text_schema;
pub(crate) use turn::init_schema as init_turn_schema;
pub(crate) use user::init_schema as init_user_schema;

// Re-export sync helpers from turn module
pub use turn::sync_helpers;
pub(crate) use text::store_content_sync;

/// Shared SQLite connection pool
///
/// This is the main entry point for SQLite storage. Create one store
/// and share it via `Arc` across all components that need database access.
///
/// Implements all storage traits:
/// - `TurnStore` - Turn/Span/Message conversation storage
/// - `ConversationStore` - Conversation-level CRUD
/// - `TextStore` - Content-addressed text storage
/// - `AssetStore` - Asset metadata storage
/// - `DocumentStore` - Document, tab, and revision storage
/// - `UserStore` - User account management
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
    pub fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        init_user_schema(&conn)?;
        init_conversation_schema(&conn)?;
        init_turn_schema(&conn)?;
        init_asset_schema(&conn)?;
        init_document_schema(&conn)?;
        init_text_schema(&conn)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_store_create() {
        let _store = SqliteStore::in_memory().unwrap();
    }
}
