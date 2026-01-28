//! SQLite implementation of CollectionStore

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Initialize collections schema
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Collections (tree organization of entities)
        CREATE TABLE IF NOT EXISTS collections (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT,
            icon TEXT,
            schema_hint TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Index for user's collections
        CREATE INDEX IF NOT EXISTS idx_collections_user ON collections(user_id);
        "#,
    )
    .context("Failed to initialize collections schema")?;
    Ok(())
}
