//! SQLite implementation of ContentBlockStore

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Initialize the content_blocks schema
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Content blocks (content-addressed text storage)
        CREATE TABLE IF NOT EXISTS content_blocks (
            id TEXT PRIMARY KEY,
            content_hash TEXT NOT NULL,
            content_type TEXT NOT NULL DEFAULT 'plain',
            text TEXT NOT NULL,
            is_private INTEGER NOT NULL DEFAULT 0,
            origin_kind TEXT,
            origin_user_id TEXT,
            origin_model_id TEXT,
            origin_source_id TEXT,
            origin_parent_id TEXT REFERENCES content_blocks(id),
            created_at INTEGER NOT NULL
        );

        -- Index for deduplication lookups
        CREATE INDEX IF NOT EXISTS idx_content_blocks_hash
            ON content_blocks(content_hash);

        -- Index for origin queries
        CREATE INDEX IF NOT EXISTS idx_content_blocks_origin
            ON content_blocks(origin_kind, origin_user_id);

        -- Index for privacy filtering
        CREATE INDEX IF NOT EXISTS idx_content_blocks_private
            ON content_blocks(is_private) WHERE is_private = 1;

        -- Index for temporal queries
        CREATE INDEX IF NOT EXISTS idx_content_blocks_created
            ON content_blocks(created_at);
        "#,
    )
    .context("Failed to initialize content_blocks schema")?;
    Ok(())
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
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='content_blocks'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify indexes exist
        let index_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_content_blocks%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_count, 4);
    }
}
