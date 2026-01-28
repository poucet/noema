//! SQLite implementation of ReferenceStore

use anyhow::Result;
use rusqlite::Connection;

/// Initialize references schema
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Cross-references between entities
        CREATE TABLE IF NOT EXISTS references (
            id TEXT PRIMARY KEY,
            from_entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            to_entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            relation_type TEXT,
            context TEXT,
            created_at INTEGER NOT NULL,
            UNIQUE(from_entity_id, to_entity_id, relation_type)
        );

        -- Index for forward lookups (outgoing references from an entity)
        CREATE INDEX IF NOT EXISTS idx_references_from ON references(from_entity_id);

        -- Index for backward lookups (backlinks to an entity)
        CREATE INDEX IF NOT EXISTS idx_references_to ON references(to_entity_id);

        -- Index for finding references by type
        CREATE INDEX IF NOT EXISTS idx_references_type ON references(relation_type) WHERE relation_type IS NOT NULL;
        "#,
    )?;
    Ok(())
}
