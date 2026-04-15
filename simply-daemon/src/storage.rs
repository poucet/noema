//! Storage backend configurations for the daemon.
//!
//! Provides concrete `StorageTypes` implementations that pair with
//! `EmbeddedDaemon`. The storage choice is orthogonal to the hosting
//! choice (embedded vs standalone).

use std::sync::Arc;
use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::traits::{StorageTypes, Stores};
use simply_core::storage::{FsBlobStore, SqliteStore};

// ---------------------------------------------------------------------------
// SQLite + filesystem blob storage
// ---------------------------------------------------------------------------

/// Storage configuration using SQLite for structured data and
/// filesystem for content-addressed blobs.
pub struct SqliteStorage;

impl StorageTypes for SqliteStorage {
    type Blob = FsBlobStore;
    type Asset = SqliteStore;
    type Text = SqliteStore;
    type Turn = SqliteStore;
    type User = SqliteStore;
    type Document = SqliteStore;
    type Entity = SqliteStore;
    type Reference = SqliteStore;
    type Collection = SqliteStore;
}

/// Store instances for SQLite storage. Shares a single connection pool
/// across all SQL-backed stores.
pub struct SqliteStores {
    sqlite: Arc<SqliteStore>,
    blob: Arc<FsBlobStore>,
}

impl SqliteStores {
    /// Get the underlying SQLite store (implements all storage traits).
    pub fn sqlite(&self) -> Arc<SqliteStore> {
        self.sqlite.clone()
    }

    /// Open storage using paths from config::PathManager.
    pub fn open() -> anyhow::Result<Self> {
        let db_path = config::PathManager::db_path()
            .ok_or_else(|| anyhow::anyhow!("Failed to determine database path"))?;
        let blob_dir = config::PathManager::blob_storage_dir()
            .ok_or_else(|| anyhow::anyhow!("Failed to determine blob storage path"))?;

        if let Some(parent) = db_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::create_dir_all(&blob_dir)?;

        // Register sqlite-vec extension before opening the connection
        simply_core::storage::register_sqlite_vec();

        let sqlite = Arc::new(SqliteStore::open(&db_path)?);
        let blob = Arc::new(FsBlobStore::new(blob_dir));
        Ok(Self { sqlite, blob })
    }

    pub fn coordinator(&self) -> StorageCoordinator<SqliteStorage> {
        StorageCoordinator::from_stores(self)
    }
}

impl Stores<SqliteStorage> for SqliteStores {
    fn turn(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn user(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn document(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn blob(&self) -> Arc<FsBlobStore> { self.blob.clone() }
    fn asset(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn text(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn entity(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn reference(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn collection(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
}
