//! SQLite implementation of VaultStore.

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::params;

use super::SqliteStore;
use crate::storage::ids::{EntityId, VaultConflictId};
use crate::storage::traits::VaultStore;
use crate::storage::types::{VaultConflict, VaultConflictReason, VaultFile, VaultSyncStatus};

fn parse_vault_file_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VaultFile> {
    let entity_id: String = row.get(0)?;
    let sync_status: String = row.get(6)?;
    Ok(VaultFile {
        entity_id: EntityId::from_string(entity_id),
        path: row.get(1)?,
        file_key: row.get(2)?,
        mtime: row.get(3)?,
        content_hash: row.get(4)?,
        frontmatter_hash: row.get(5)?,
        sync_status: VaultSyncStatus::from(sync_status),
        last_seen_at: row.get(7)?,
    })
}

fn parse_vault_conflict_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VaultConflict> {
    let id: String = row.get(0)?;
    let entity_id: Option<String> = row.get(1)?;
    let reason: String = row.get(3)?;
    let observed_entity_id: Option<String> = row.get(4)?;
    let details: Option<String> = row.get(5)?;
    Ok(VaultConflict {
        id: VaultConflictId::from_string(id),
        entity_id: entity_id.map(EntityId::from_string),
        path: row.get(2)?,
        reason: VaultConflictReason::from(reason),
        observed_entity_id: observed_entity_id.map(EntityId::from_string),
        details: details.and_then(|d| serde_json::from_str(&d).ok()),
        created_at: row.get(6)?,
    })
}

fn vault_file_columns() -> &'static str {
    "entity_id, path, file_key, mtime, content_hash, frontmatter_hash, sync_status, last_seen_at"
}

fn vault_conflict_columns() -> &'static str {
    "id, entity_id, path, reason, observed_entity_id, details, created_at"
}

#[async_trait]
impl VaultStore for SqliteStore {
    async fn upsert_vault_file(&self, file: &VaultFile) -> Result<()> {
        let conn = self.write_conn().lock().unwrap();
        conn.execute(
            "INSERT INTO vault_files \
                (entity_id, path, file_key, mtime, content_hash, frontmatter_hash, sync_status, last_seen_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(entity_id) DO UPDATE SET \
                path = excluded.path, \
                file_key = excluded.file_key, \
                mtime = excluded.mtime, \
                content_hash = excluded.content_hash, \
                frontmatter_hash = excluded.frontmatter_hash, \
                sync_status = excluded.sync_status, \
                last_seen_at = excluded.last_seen_at",
            params![
                file.entity_id.as_str(),
                file.path.as_str(),
                file.file_key.as_deref(),
                file.mtime,
                file.content_hash.as_str(),
                file.frontmatter_hash.as_deref(),
                file.sync_status.as_str(),
                file.last_seen_at,
            ],
        )
        .context("Failed to upsert vault file")?;
        Ok(())
    }

    async fn get_vault_file(&self, entity_id: &EntityId) -> Result<Option<VaultFile>> {
        let conn = self.read_conn().lock().unwrap();
        let sql = format!(
            "SELECT {} FROM vault_files WHERE entity_id = ?1",
            vault_file_columns()
        );
        match conn.query_row(&sql, params![entity_id.as_str()], parse_vault_file_row) {
            Ok(file) => Ok(Some(file)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e).context("Failed to get vault file"),
        }
    }

    async fn get_vault_file_by_path(&self, path: &str) -> Result<Option<VaultFile>> {
        let conn = self.read_conn().lock().unwrap();
        let sql = format!(
            "SELECT {} FROM vault_files WHERE path = ?1",
            vault_file_columns()
        );
        match conn.query_row(&sql, params![path], parse_vault_file_row) {
            Ok(file) => Ok(Some(file)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e).context("Failed to get vault file by path"),
        }
    }

    async fn list_vault_files(&self, status: Option<&VaultSyncStatus>) -> Result<Vec<VaultFile>> {
        let conn = self.read_conn().lock().unwrap();
        let sql = match status {
            Some(_) => format!(
                "SELECT {} FROM vault_files WHERE sync_status = ?1 ORDER BY path ASC",
                vault_file_columns()
            ),
            None => format!(
                "SELECT {} FROM vault_files ORDER BY path ASC",
                vault_file_columns()
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let files = match status {
            Some(status) => stmt
                .query_map(params![status.as_str()], parse_vault_file_row)?
                .filter_map(|row| row.ok())
                .collect(),
            None => stmt
                .query_map([], parse_vault_file_row)?
                .filter_map(|row| row.ok())
                .collect(),
        };
        Ok(files)
    }

    async fn delete_vault_file(&self, entity_id: &EntityId) -> Result<()> {
        let conn = self.write_conn().lock().unwrap();
        conn.execute(
            "DELETE FROM vault_files WHERE entity_id = ?1",
            params![entity_id.as_str()],
        )
        .context("Failed to delete vault file")?;
        Ok(())
    }

    async fn insert_vault_conflict(&self, conflict: &VaultConflict) -> Result<()> {
        let conn = self.write_conn().lock().unwrap();
        let details_json = conflict.details.as_ref().map(|d| d.to_string());
        conn.execute(
            "INSERT OR REPLACE INTO vault_conflicts \
                (id, entity_id, path, reason, observed_entity_id, details, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                conflict.id.as_str(),
                conflict.entity_id.as_ref().map(|id| id.as_str()),
                conflict.path.as_str(),
                conflict.reason.as_str(),
                conflict.observed_entity_id.as_ref().map(|id| id.as_str()),
                details_json,
                conflict.created_at,
            ],
        )
        .context("Failed to insert vault conflict")?;
        Ok(())
    }

    async fn list_vault_conflicts(&self) -> Result<Vec<VaultConflict>> {
        let conn = self.read_conn().lock().unwrap();
        let sql = format!(
            "SELECT {} FROM vault_conflicts ORDER BY created_at DESC, path ASC",
            vault_conflict_columns()
        );
        let mut stmt = conn.prepare(&sql)?;
        let conflicts = stmt
            .query_map([], parse_vault_conflict_row)?
            .filter_map(|row| row.ok())
            .collect();
        Ok(conflicts)
    }

    async fn delete_vault_conflict(&self, id: &VaultConflictId) -> Result<bool> {
        let conn = self.write_conn().lock().unwrap();
        let affected = conn
            .execute(
                "DELETE FROM vault_conflicts WHERE id = ?1",
                params![id.as_str()],
            )
            .context("Failed to delete vault conflict")?;
        Ok(affected > 0)
    }

    async fn clear_vault_conflicts_for_path(&self, path: &str) -> Result<()> {
        let conn = self.write_conn().lock().unwrap();
        conn.execute("DELETE FROM vault_conflicts WHERE path = ?1", params![path])
            .context("Failed to clear vault conflicts for path")?;
        Ok(())
    }
}
