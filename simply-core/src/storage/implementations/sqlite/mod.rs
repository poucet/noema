//! SQLite storage backend
//!
//! Provides `SqliteStore` - a shared SQLite connection wrapper that
//! implements all storage traits for conversation, asset, document,
//! content block, and user management.
//!
//! All trait implementations are in submodules:
//! - `asset` - AssetStore impl
//! - `text` - TextStore impl
//! - `turn` - TurnStore impl
//! - `document` - DocumentStore impl
//! - `entity` - EntityStore impl
//! - `user` - UserStore impl

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

// Submodules with trait implementations
mod asset;
mod collection;
mod document;
mod entity;
mod reference;
mod temporal;
mod text;
mod turn;
mod user;

// Re-export init_schema functions for use in SqliteStore::init_schema
pub(crate) use asset::init_schema as init_asset_schema;
pub(crate) use collection::init_schema as init_collection_schema;
pub(crate) use document::init_schema as init_document_schema;
pub(crate) use entity::init_schema as init_entity_schema;
pub(crate) use reference::init_schema as init_reference_schema;
pub(crate) use temporal::init_schema as init_temporal_schema;
pub(crate) use text::init_schema as init_text_schema;
pub(crate) use turn::init_schema as init_turn_schema;
pub(crate) use user::init_schema as init_user_schema;

/// Shared SQLite connection pool
///
/// This is the main entry point for SQLite storage. Create one store
/// and share it via `Arc` across all components that need database access.
///
/// Implements all storage traits:
/// - `TurnStore` - Turn/Span/Message conversation storage
/// - `EntityStore` - Unified addressable layer (conversations, documents, assets)
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
        init_entity_schema(&conn)?;
        init_turn_schema(&conn)?;
        init_asset_schema(&conn)?;
        init_document_schema(&conn)?;
        init_text_schema(&conn)?;
        init_reference_schema(&conn)?;
        init_collection_schema(&conn)?;
        init_temporal_schema(&conn)?;
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
