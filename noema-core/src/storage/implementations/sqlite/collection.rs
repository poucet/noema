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

        -- Collection items (entities organized in tree structure)
        CREATE TABLE IF NOT EXISTS collection_items (
            id TEXT PRIMARY KEY,
            collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
            target_type TEXT NOT NULL,
            target_id TEXT NOT NULL,
            parent_item_id TEXT REFERENCES collection_items(id) ON DELETE CASCADE,
            position INTEGER NOT NULL,
            name_override TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Indexes for collection items
        CREATE INDEX IF NOT EXISTS idx_collection_items_collection ON collection_items(collection_id);
        CREATE INDEX IF NOT EXISTS idx_collection_items_parent ON collection_items(parent_item_id);
        CREATE INDEX IF NOT EXISTS idx_collection_items_target ON collection_items(target_type, target_id);
        "#,
    )
    .context("Failed to initialize collections schema")?;
    Ok(())
}
