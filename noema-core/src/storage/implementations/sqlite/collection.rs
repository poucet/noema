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

        -- Item fields (typed metadata on collection items)
        CREATE TABLE IF NOT EXISTS item_fields (
            id TEXT PRIMARY KEY,
            item_id TEXT NOT NULL REFERENCES collection_items(id) ON DELETE CASCADE,
            field_name TEXT NOT NULL,
            field_type TEXT NOT NULL,
            value_text TEXT,
            value_number REAL,
            value_boolean INTEGER,
            value_json TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            UNIQUE(item_id, field_name)
        );

        -- Indexes for item fields
        CREATE INDEX IF NOT EXISTS idx_item_fields_item ON item_fields(item_id);
        CREATE INDEX IF NOT EXISTS idx_item_fields_name ON item_fields(field_name);

        -- Item tags (cross-cutting organization)
        CREATE TABLE IF NOT EXISTS item_tags (
            item_id TEXT NOT NULL REFERENCES collection_items(id) ON DELETE CASCADE,
            tag TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            PRIMARY KEY(item_id, tag)
        );

        -- Index for finding items by tag
        CREATE INDEX IF NOT EXISTS idx_item_tags_tag ON item_tags(tag);

        -- Collection views (saved view configurations)
        CREATE TABLE IF NOT EXISTS collection_views (
            id TEXT PRIMARY KEY,
            collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            view_type TEXT NOT NULL,
            config TEXT NOT NULL,
            is_default INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Index for collection's views
        CREATE INDEX IF NOT EXISTS idx_collection_views_collection ON collection_views(collection_id);
        "#,
    )
    .context("Failed to initialize collections schema")?;
    Ok(())
}
