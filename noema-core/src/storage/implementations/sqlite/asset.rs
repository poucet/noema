//! SQLite implementation of AssetStore

use anyhow::Result;
use async_trait::async_trait;
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::SqliteStore;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::AssetId;
use crate::storage::traits::AssetStore;
use crate::storage::types::{Asset, StoredAsset};

pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS assets (
            id TEXT PRIMARY KEY,
            blob_hash TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            local_path TEXT,
            is_private INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_assets_blob_hash ON assets(blob_hash);
        CREATE INDEX IF NOT EXISTS idx_assets_private ON assets(is_private);
        CREATE INDEX IF NOT EXISTS idx_assets_created ON assets(created_at);
        "#,
    )?;
    Ok(())
}

#[async_trait]
impl AssetStore for SqliteStore {
    async fn create_asset(&self, asset: Asset) -> Result<AssetId> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        let id = AssetId::from_string(Uuid::new_v4().to_string());

        conn.execute(
            "INSERT INTO assets (id, blob_hash, mime_type, size_bytes, local_path, is_private, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id.as_str(),
                asset.blob_hash,
                asset.mime_type,
                asset.size_bytes,
                asset.local_path,
                asset.is_private as i32,
                now
            ],
        )?;

        Ok(id)
    }

    async fn get(&self, id: &AssetId) -> Result<Option<StoredAsset>> {
        let conn = self.conn().lock().unwrap();
        let asset = conn
            .query_row(
                "SELECT blob_hash, mime_type, size_bytes, local_path, is_private, created_at
                 FROM assets WHERE id = ?1",
                params![id.as_str()],
                |row| {
                    Ok(StoredAsset {
                        id: id.clone(),
                        asset: Asset {
                            blob_hash: row.get(0)?,
                            mime_type: row.get(1)?,
                            size_bytes: row.get(2)?,
                            local_path: row.get(3)?,
                            is_private: row.get::<_, i32>(4)? != 0,
                        },
                        created_at: row.get(5)?,
                    })
                },
            )
            .ok();
        Ok(asset)
    }

    async fn exists(&self, id: &AssetId) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let exists = conn
            .query_row(
                "SELECT 1 FROM assets WHERE id = ?1",
                params![id.as_str()],
                |_| Ok(true),
            )
            .unwrap_or(false);
        Ok(exists)
    }

    async fn delete(&self, id: &AssetId) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let deleted = conn.execute("DELETE FROM assets WHERE id = ?1", params![id.as_str()])?;
        Ok(deleted > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_schema_creation() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        // Verify table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='assets'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify indexes exist
        let index_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_assets%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_count, 3);
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let store = SqliteStore::in_memory().unwrap();

        let asset = Asset::new("abc123hash", "image/png", 1024);

        let id = store.create_asset(asset).await.unwrap();

        let stored = store.get(&id).await.unwrap().unwrap();
        assert_eq!(stored.blob_hash(), "abc123hash");
        assert_eq!(stored.mime_type(), "image/png");
        assert_eq!(stored.size_bytes(), 1024);
        assert!(!stored.is_private());
    }

    #[tokio::test]
    async fn test_same_blob_different_assets() {
        let store = SqliteStore::in_memory().unwrap();

        // Two assets with the same blob hash should get different IDs
        let asset1 = Asset::new("same_blob_hash", "image/jpeg", 2048);
        let id1 = store.create_asset(asset1).await.unwrap();

        let asset2 = Asset::new("same_blob_hash", "image/jpeg", 2048);
        let id2 = store.create_asset(asset2).await.unwrap();

        // IDs should be different
        assert_ne!(id1.as_str(), id2.as_str());

        // Both should exist and have the same blob_hash
        let stored1 = store.get(&id1).await.unwrap().unwrap();
        let stored2 = store.get(&id2).await.unwrap().unwrap();
        assert_eq!(stored1.blob_hash(), stored2.blob_hash());
    }

    #[tokio::test]
    async fn test_exists() {
        let store = SqliteStore::in_memory().unwrap();

        let asset = Asset::new("exists_test_hash", "audio/mp3", 4096);
        let id = store.create_asset(asset).await.unwrap();

        assert!(store.exists(&id).await.unwrap());
        assert!(!store.exists(&AssetId::from_string("nonexistent".to_string())).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let store = SqliteStore::in_memory().unwrap();

        let asset = Asset::new("delete_test_hash", "application/pdf", 8192);
        let id = store.create_asset(asset).await.unwrap();

        assert!(store.exists(&id).await.unwrap());
        assert!(store.delete(&id).await.unwrap());
        assert!(!store.exists(&id).await.unwrap());

        // Delete non-existent returns false
        assert!(!store.delete(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_private_asset() {
        let store = SqliteStore::in_memory().unwrap();

        let asset = Asset::new("private_hash", "image/png", 512).private();
        let id = store.create_asset(asset).await.unwrap();

        let stored = store.get(&id).await.unwrap().unwrap();
        assert!(stored.is_private());
    }

    #[tokio::test]
    async fn test_local_path() {
        let store = SqliteStore::in_memory().unwrap();

        let asset = Asset::new("local_path_hash", "image/png", 256)
            .with_local_path("/home/user/photos/photo.png");

        let id = store.create_asset(asset).await.unwrap();

        let stored = store.get(&id).await.unwrap().unwrap();
        assert_eq!(stored.local_path(), Some("/home/user/photos/photo.png"));
    }
}
