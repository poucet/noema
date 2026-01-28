//! SQLite implementation of TemporalStore
//!
//! Provides time-range queries and activity summaries for LLM context.

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Initialize temporal indexes for efficient time-range queries
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Temporal indexes for time-range queries

        -- Content blocks: query by creation time
        CREATE INDEX IF NOT EXISTS idx_content_blocks_created ON content_blocks(created_at);

        -- Messages: query by creation time
        CREATE INDEX IF NOT EXISTS idx_ucm_messages_created ON ucm_messages(created_at);

        -- Turns: query by creation time
        CREATE INDEX IF NOT EXISTS idx_turns_created ON turns(created_at);

        -- Spans: query by creation time
        CREATE INDEX IF NOT EXISTS idx_ucm_spans_created ON ucm_spans(created_at);

        -- Document revisions: query by creation time
        CREATE INDEX IF NOT EXISTS idx_document_revisions_created ON document_revisions(created_at);

        -- Entities: query by creation and update time
        CREATE INDEX IF NOT EXISTS idx_entities_created ON entities(created_at);
        CREATE INDEX IF NOT EXISTS idx_entities_updated ON entities(updated_at);

        -- Collections: query by update time (for recent activity)
        CREATE INDEX IF NOT EXISTS idx_collections_updated ON collections(updated_at);

        -- Collection items: query by update time
        CREATE INDEX IF NOT EXISTS idx_collection_items_updated ON collection_items(updated_at);
        "#,
    )
    .context("Failed to initialize temporal indexes")?;
    Ok(())
}
