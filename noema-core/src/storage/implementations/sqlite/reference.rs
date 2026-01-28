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
// Helper Functions
// ============================================================================

/// Parse a reference from a database row
fn parse_reference(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredReference> {
    let id: ReferenceId = row.get(0)?;
    let from_entity_id: EntityId = row.get(1)?;
    let to_entity_id: EntityId = row.get(2)?;
    let relation_type: Option<String> = row.get(3)?;
    let context: Option<String> = row.get(4)?;
    let created_at: i64 = row.get(5)?;

    let reference = Reference {
        from_entity_id,
        to_entity_id,
        relation_type: relation_type.map(RelationType::from),
        context,
    };

    Ok(stored(id, reference, created_at))
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

    async fn get_outgoing(&self, entity_id: &EntityId) -> Result<Vec<StoredReference>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, from_entity_id, to_entity_id, relation_type, context, created_at
             FROM references WHERE from_entity_id = ?1
             ORDER BY created_at DESC"
        )?;

        let refs = stmt
            .query_map(params![entity_id.as_str()], parse_reference)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(refs)
    }

    async fn get_outgoing_by_type(
        &self,
        entity_id: &EntityId,
        relation_type: &RelationType,
    ) -> Result<Vec<StoredReference>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, from_entity_id, to_entity_id, relation_type, context, created_at
             FROM references WHERE from_entity_id = ?1 AND relation_type = ?2
             ORDER BY created_at DESC"
        )?;

        let refs = stmt
            .query_map(params![entity_id.as_str(), relation_type.as_str()], parse_reference)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(refs)
    }

    async fn get_backlinks(&self, entity_id: &EntityId) -> Result<Vec<StoredReference>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, from_entity_id, to_entity_id, relation_type, context, created_at
             FROM references WHERE to_entity_id = ?1
             ORDER BY created_at DESC"
        )?;

        let refs = stmt
            .query_map(params![entity_id.as_str()], parse_reference)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(refs)
    }

    async fn get_backlinks_by_type(
        &self,
        entity_id: &EntityId,
        relation_type: &RelationType,
    ) -> Result<Vec<StoredReference>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, from_entity_id, to_entity_id, relation_type, context, created_at
             FROM references WHERE to_entity_id = ?1 AND relation_type = ?2
             ORDER BY created_at DESC"
        )?;

        let refs = stmt
            .query_map(params![entity_id.as_str(), relation_type.as_str()], parse_reference)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(refs)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::traits::EntityStore;
    use crate::storage::types::EntityType;

    async fn setup_store_with_entities() -> (SqliteStore, EntityId, EntityId, EntityId) {
        let store = SqliteStore::in_memory().unwrap();

        // Create test entities
        let entity_a = store
            .create_entity(EntityType::conversation(), None)
            .await
            .unwrap();
        let entity_b = store
            .create_entity(EntityType::document(), None)
            .await
            .unwrap();
        let entity_c = store
            .create_entity(EntityType::asset(), None)
            .await
            .unwrap();

        (store, entity_a, entity_b, entity_c)
    }

    #[tokio::test]
    async fn test_create_reference() {
        let (store, entity_a, entity_b, _) = setup_store_with_entities().await;

        let ref_id = store
            .create_reference(
                &entity_a,
                &entity_b,
                Some(&RelationType::new("cites")),
                Some("See document for details"),
            )
            .await
            .unwrap();

        assert!(store.reference_exists(&entity_a, &entity_b, Some(&RelationType::new("cites"))).await.unwrap());
        assert!(!ref_id.as_str().is_empty());
    }

    #[tokio::test]
    async fn test_create_reference_without_type() {
        let (store, entity_a, entity_b, _) = setup_store_with_entities().await;

        store
            .create_reference(&entity_a, &entity_b, None, None)
            .await
            .unwrap();

        assert!(store.reference_exists(&entity_a, &entity_b, None).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_reference() {
        let (store, entity_a, entity_b, _) = setup_store_with_entities().await;

        let ref_id = store
            .create_reference(&entity_a, &entity_b, None, None)
            .await
            .unwrap();

        assert!(store.delete_reference(&ref_id).await.unwrap());
        assert!(!store.reference_exists(&entity_a, &entity_b, None).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_references_between() {
        let (store, entity_a, entity_b, _) = setup_store_with_entities().await;

        // Create multiple references between same entities
        store
            .create_reference(&entity_a, &entity_b, Some(&RelationType::new("cites")), None)
            .await
            .unwrap();
        store
            .create_reference(&entity_a, &entity_b, Some(&RelationType::new("mentions")), None)
            .await
            .unwrap();

        let deleted = store.delete_references_between(&entity_a, &entity_b).await.unwrap();
        assert_eq!(deleted, 2);

        assert!(!store.reference_exists(&entity_a, &entity_b, Some(&RelationType::new("cites"))).await.unwrap());
        assert!(!store.reference_exists(&entity_a, &entity_b, Some(&RelationType::new("mentions"))).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_outgoing() {
        let (store, entity_a, entity_b, entity_c) = setup_store_with_entities().await;

        // A references B and C
        store
            .create_reference(&entity_a, &entity_b, Some(&RelationType::new("cites")), None)
            .await
            .unwrap();
        store
            .create_reference(&entity_a, &entity_c, Some(&RelationType::new("mentions")), None)
            .await
            .unwrap();

        let outgoing = store.get_outgoing(&entity_a).await.unwrap();
        assert_eq!(outgoing.len(), 2);
    }

    #[tokio::test]
    async fn test_get_outgoing_by_type() {
        let (store, entity_a, entity_b, entity_c) = setup_store_with_entities().await;

        store
            .create_reference(&entity_a, &entity_b, Some(&RelationType::new("cites")), None)
            .await
            .unwrap();
        store
            .create_reference(&entity_a, &entity_c, Some(&RelationType::new("mentions")), None)
            .await
            .unwrap();

        let cites_only = store
            .get_outgoing_by_type(&entity_a, &RelationType::new("cites"))
            .await
            .unwrap();
        assert_eq!(cites_only.len(), 1);
        assert_eq!(cites_only[0].to_entity_id, entity_b);
    }

    #[tokio::test]
    async fn test_get_backlinks() {
        let (store, entity_a, entity_b, entity_c) = setup_store_with_entities().await;

        // A and B both reference C
        store
            .create_reference(&entity_a, &entity_c, Some(&RelationType::new("cites")), None)
            .await
            .unwrap();
        store
            .create_reference(&entity_b, &entity_c, Some(&RelationType::new("mentions")), None)
            .await
            .unwrap();

        let backlinks = store.get_backlinks(&entity_c).await.unwrap();
        assert_eq!(backlinks.len(), 2);
    }

    #[tokio::test]
    async fn test_get_backlinks_by_type() {
        let (store, entity_a, entity_b, entity_c) = setup_store_with_entities().await;

        store
            .create_reference(&entity_a, &entity_c, Some(&RelationType::new("cites")), None)
            .await
            .unwrap();
        store
            .create_reference(&entity_b, &entity_c, Some(&RelationType::new("mentions")), None)
            .await
            .unwrap();

        let cites_only = store
            .get_backlinks_by_type(&entity_c, &RelationType::new("cites"))
            .await
            .unwrap();
        assert_eq!(cites_only.len(), 1);
        assert_eq!(cites_only[0].from_entity_id, entity_a);
    }

    #[tokio::test]
    async fn test_reference_with_context() {
        let (store, entity_a, entity_b, _) = setup_store_with_entities().await;

        store
            .create_reference(
                &entity_a,
                &entity_b,
                Some(&RelationType::new("mentions")),
                Some("@api-design"),
            )
            .await
            .unwrap();

        let outgoing = store.get_outgoing(&entity_a).await.unwrap();
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].context.as_deref(), Some("@api-design"));
    }

    #[tokio::test]
    async fn test_unique_constraint() {
        let (store, entity_a, entity_b, _) = setup_store_with_entities().await;

        // First reference succeeds
        store
            .create_reference(&entity_a, &entity_b, Some(&RelationType::new("cites")), None)
            .await
            .unwrap();

        // Duplicate reference should fail
        let result = store
            .create_reference(&entity_a, &entity_b, Some(&RelationType::new("cites")), None)
            .await;
        assert!(result.is_err());
    }
}
