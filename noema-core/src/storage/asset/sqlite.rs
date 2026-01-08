//! SQLite implementation of AssetStore

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::{AssetInfo, AssetStore};
use crate::storage::session::SqliteStore;
use crate::storage::helper::unix_timestamp;

pub (crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
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
        "#,
    )
    .context("Failed to initialize asset schema")?;
    Ok(())
}

#[async_trait]
impl AssetStore for SqliteStore {
    async fn register_asset(
        &self,
        hash: &str,
        mime_type: &str,
        original_filename: Option<&str>,
        file_size_bytes: Option<i64>,
        local_path: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "INSERT OR IGNORE INTO assets (id, mime_type, original_filename, file_size_bytes, local_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![hash, mime_type, original_filename, file_size_bytes, local_path, now],
        )?;
        Ok(())
    }

    async fn get_asset(&self, hash: &str) -> Result<Option<AssetInfo>> {
        let conn = self.conn().lock().unwrap();
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
}
