//! Temporal indexes for efficient time-range queries
//!
//! Provides indexes on created_at/updated_at columns for efficient
//! time-range queries via `EntityStore::list_entities_in_range`.

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Initialize temporal indexes for efficient time-range queries
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Temporal indexes for time-range queries on entities table
        CREATE INDEX IF NOT EXISTS idx_entities_created ON entities(created_at);
        CREATE INDEX IF NOT EXISTS idx_entities_updated ON entities(updated_at);
        CREATE INDEX IF NOT EXISTS idx_entities_user_updated ON entities(user_id, updated_at);
        "#,
    )
    .context("Failed to initialize temporal indexes")?;
    Ok(())
}
