//! SQLite implementation of ReferenceStore

use anyhow::Result;
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{EntityId, ReferenceId};
use crate::storage::traits::{ReferenceStore, StoredReference};
use crate::storage::types::{stored, Reference, RelationType};

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

// ============================================================================
// ReferenceStore Implementation
// ============================================================================

#[async_trait]
impl ReferenceStore for SqliteStore {
    async fn create_reference(
        &self,
        from_entity_id: &EntityId,
        to_entity_id: &EntityId,
        relation_type: Option<&RelationType>,
        context: Option<&str>,
    ) -> Result<ReferenceId> {
        let conn = self.conn().lock().unwrap();
        let id = ReferenceId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO references (id, from_entity_id, to_entity_id, relation_type, context, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.as_str(),
                from_entity_id.as_str(),
                to_entity_id.as_str(),
                relation_type.map(|r| r.as_str()),
                context,
                now
            ],
        )?;

        Ok(id)
    }

    async fn delete_reference(&self, id: &ReferenceId) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM references WHERE id = ?1",
            params![id.as_str()],
        )?;
        Ok(rows > 0)
    }

    async fn delete_references_between(
        &self,
        from_entity_id: &EntityId,
        to_entity_id: &EntityId,
    ) -> Result<usize> {
        let conn = self.conn().lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM references WHERE from_entity_id = ?1 AND to_entity_id = ?2",
            params![from_entity_id.as_str(), to_entity_id.as_str()],
        )?;
        Ok(rows)
    }

    async fn get_outgoing(&self, _entity_id: &EntityId) -> Result<Vec<StoredReference>> {
        todo!("Implement in 3.6.5")
    }

    async fn get_outgoing_by_type(
        &self,
        _entity_id: &EntityId,
        _relation_type: &RelationType,
    ) -> Result<Vec<StoredReference>> {
        todo!("Implement in 3.6.5")
    }

    async fn get_backlinks(&self, _entity_id: &EntityId) -> Result<Vec<StoredReference>> {
        todo!("Implement in 3.6.6")
    }

    async fn get_backlinks_by_type(
        &self,
        _entity_id: &EntityId,
        _relation_type: &RelationType,
    ) -> Result<Vec<StoredReference>> {
        todo!("Implement in 3.6.6")
    }

    async fn reference_exists(
        &self,
        from_entity_id: &EntityId,
        to_entity_id: &EntityId,
        relation_type: Option<&RelationType>,
    ) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let exists: bool = match relation_type {
            Some(rt) => conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM references WHERE from_entity_id = ?1 AND to_entity_id = ?2 AND relation_type = ?3)",
                params![from_entity_id.as_str(), to_entity_id.as_str(), rt.as_str()],
                |row| row.get(0),
            )?,
            None => conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM references WHERE from_entity_id = ?1 AND to_entity_id = ?2 AND relation_type IS NULL)",
                params![from_entity_id.as_str(), to_entity_id.as_str()],
                |row| row.get(0),
            )?,
        };
        Ok(exists)
    }
}
