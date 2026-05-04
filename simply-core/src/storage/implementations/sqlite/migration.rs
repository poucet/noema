//! SQLite schema migration runner.
//!
//! The early SQLite stores still create their current schema from code. The
//! migration table records that schema as the baseline, then applies additive
//! migrations for user-data-safe changes from this point forward.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::storage::helper::unix_timestamp;

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const BASELINE_VERSION: i64 = 1;
const BASELINE_NAME: &str = "code_defined_schema_baseline";

const MIGRATIONS: &[Migration] = &[Migration {
    version: 2,
    name: "vault_projection_tables",
    sql: r#"
        CREATE TABLE IF NOT EXISTS vault_files (
            entity_id TEXT PRIMARY KEY REFERENCES entities(id) ON DELETE CASCADE,
            path TEXT NOT NULL UNIQUE,
            file_key TEXT,
            mtime INTEGER,
            content_hash TEXT NOT NULL,
            frontmatter_hash TEXT,
            sync_status TEXT NOT NULL,
            last_seen_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_vault_files_status
            ON vault_files(sync_status);

        CREATE INDEX IF NOT EXISTS idx_vault_files_last_seen
            ON vault_files(last_seen_at);

        CREATE TABLE IF NOT EXISTS vault_conflicts (
            id TEXT PRIMARY KEY,
            entity_id TEXT,
            path TEXT NOT NULL,
            reason TEXT NOT NULL,
            observed_entity_id TEXT,
            details TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_vault_conflicts_entity
            ON vault_conflicts(entity_id);

        CREATE INDEX IF NOT EXISTS idx_vault_conflicts_reason
            ON vault_conflicts(reason);

        CREATE INDEX IF NOT EXISTS idx_vault_conflicts_path
            ON vault_conflicts(path);
        "#,
}];

pub(crate) fn run_migrations(conn: &Connection) -> Result<()> {
    init_schema_migrations(conn)?;
    record_baseline_if_needed(conn)?;

    for migration in MIGRATIONS {
        if !has_migration(conn, migration.version)? {
            apply_migration(conn, migration)?;
        }
    }

    Ok(())
}

fn init_schema_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at INTEGER NOT NULL
        );
        "#,
    )
    .context("Failed to initialize schema_migrations table")?;
    Ok(())
}

fn record_baseline_if_needed(conn: &Connection) -> Result<()> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .context("Failed to inspect schema_migrations")?;

    if count == 0 {
        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            params![BASELINE_VERSION, BASELINE_NAME, unix_timestamp()],
        )
        .context("Failed to record SQLite schema baseline")?;
    }

    Ok(())
}

fn has_migration(conn: &Connection, version: i64) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
            params![version],
            |row| row.get(0),
        )
        .with_context(|| format!("Failed to inspect migration {version}"))?;
    Ok(count > 0)
}

fn apply_migration(conn: &Connection, migration: &Migration) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .with_context(|| format!("Failed to begin migration {}", migration.version))?;

    let result = (|| -> Result<()> {
        conn.execute_batch(migration.sql)
            .with_context(|| format!("Failed to apply migration {}", migration.version))?;
        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            params![migration.version, migration.name, unix_timestamp()],
        )
        .with_context(|| format!("Failed to record migration {}", migration.version))?;
        Ok(())
    })();

    match result {
        Ok(()) => conn
            .execute_batch("COMMIT")
            .with_context(|| format!("Failed to commit migration {}", migration.version)),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}
