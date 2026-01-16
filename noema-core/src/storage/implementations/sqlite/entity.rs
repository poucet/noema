//! SQLite implementation of EntityStore

use anyhow::Result;
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{EntityId, UserId};
use crate::storage::traits::{EntityStore, StoredEntity};
use crate::storage::types::entity::{Entity, EntityRelation, EntityType, RelationType};
use crate::storage::types::stored_editable;

/// Initialize entity schema (entities and entity_relations tables)
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Entities: unified addressable layer
        CREATE TABLE IF NOT EXISTS entities (
            id TEXT PRIMARY KEY,
            entity_type TEXT NOT NULL,
            user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
            name TEXT,
            slug TEXT UNIQUE,
            is_private INTEGER NOT NULL DEFAULT 1,
            is_archived INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_entities_user ON entities(user_id);
        CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type, user_id);
        CREATE INDEX IF NOT EXISTS idx_entities_slug ON entities(slug) WHERE slug IS NOT NULL;

        -- Entity relations: relationships between entities
        CREATE TABLE IF NOT EXISTS entity_relations (
            from_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            to_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            relation TEXT NOT NULL,
            metadata TEXT,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (from_id, to_id, relation)
        );

        CREATE INDEX IF NOT EXISTS idx_entity_relations_to ON entity_relations(to_id, relation);
        "#,
    )?;
    Ok(())
}

// ============================================================================
// EntityStore Implementation
// ============================================================================

#[async_trait]
impl EntityStore for SqliteStore {
    async fn create_entity(
        &self,
        entity_type: EntityType,
        user_id: Option<&UserId>,
    ) -> Result<EntityId> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        let entity_id = EntityId::new();

        conn.execute(
            "INSERT INTO entities (id, entity_type, user_id, is_private, is_archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, 0, ?4, ?5)",
            params![
                entity_id.as_str(),
                entity_type.as_str(),
                user_id.map(|u| u.as_str()),
                now,
                now
            ],
        )?;

        Ok(entity_id)
    }

    async fn get_entity(&self, id: &EntityId) -> Result<Option<StoredEntity>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at
             FROM entities WHERE id = ?1",
            params![id.as_str()],
            |row| {
                let id: String = row.get(0)?;
                let entity_type: String = row.get(1)?;
                let user_id: Option<String> = row.get(2)?;
                let name: Option<String> = row.get(3)?;
                let slug: Option<String> = row.get(4)?;
                let is_private: i32 = row.get(5)?;
                let is_archived: i32 = row.get(6)?;
                let created_at: i64 = row.get(7)?;
                let updated_at: i64 = row.get(8)?;
                Ok((id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at))
            },
        );

        match result {
            Ok((id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at)) => {
                let entity = Entity {
                    entity_type: EntityType::new(entity_type),
                    user_id: user_id.map(UserId::from_string),
                    name,
                    slug,
                    is_private: is_private != 0,
                    is_archived: is_archived != 0,
                };
                Ok(Some(stored_editable(EntityId::from_string(id), entity, created_at, updated_at)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_entity_by_slug(&self, slug: &str) -> Result<Option<StoredEntity>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at
             FROM entities WHERE slug = ?1",
            params![slug],
            |row| {
                let id: String = row.get(0)?;
                let entity_type: String = row.get(1)?;
                let user_id: Option<String> = row.get(2)?;
                let name: Option<String> = row.get(3)?;
                let slug: Option<String> = row.get(4)?;
                let is_private: i32 = row.get(5)?;
                let is_archived: i32 = row.get(6)?;
                let created_at: i64 = row.get(7)?;
                let updated_at: i64 = row.get(8)?;
                Ok((id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at))
            },
        );

        match result {
            Ok((id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at)) => {
                let entity = Entity {
                    entity_type: EntityType::new(entity_type),
                    user_id: user_id.map(UserId::from_string),
                    name,
                    slug,
                    is_private: is_private != 0,
                    is_archived: is_archived != 0,
                };
                Ok(Some(stored_editable(EntityId::from_string(id), entity, created_at, updated_at)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn list_entities(
        &self,
        user_id: &UserId,
        entity_type: Option<&EntityType>,
    ) -> Result<Vec<StoredEntity>> {
        let conn = self.conn().lock().unwrap();

        let entity_type_str = entity_type.map(|et| et.as_str().to_string());

        let entities: Vec<StoredEntity> = match &entity_type_str {
            Some(et_str) => {
                let mut stmt = conn.prepare(
                    "SELECT id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at
                     FROM entities
                     WHERE user_id = ?1 AND entity_type = ?2 AND is_archived = 0
                     ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map(params![user_id.as_str(), et_str], |row| {
                    let id: String = row.get(0)?;
                    let entity_type: String = row.get(1)?;
                    let user_id: Option<String> = row.get(2)?;
                    let name: Option<String> = row.get(3)?;
                    let slug: Option<String> = row.get(4)?;
                    let is_private: i32 = row.get(5)?;
                    let is_archived: i32 = row.get(6)?;
                    let created_at: i64 = row.get(7)?;
                    let updated_at: i64 = row.get(8)?;
                    Ok((id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at))
                })?;
                rows.filter_map(|r| r.ok())
                    .map(|(id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at)| {
                        let entity = Entity {
                            entity_type: EntityType::new(entity_type),
                            user_id: user_id.map(UserId::from_string),
                            name,
                            slug,
                            is_private: is_private != 0,
                            is_archived: is_archived != 0,
                        };
                        stored_editable(EntityId::from_string(id), entity, created_at, updated_at)
                    })
                    .collect()
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at
                     FROM entities
                     WHERE user_id = ?1 AND is_archived = 0
                     ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map(params![user_id.as_str()], |row| {
                    let id: String = row.get(0)?;
                    let entity_type: String = row.get(1)?;
                    let user_id: Option<String> = row.get(2)?;
                    let name: Option<String> = row.get(3)?;
                    let slug: Option<String> = row.get(4)?;
                    let is_private: i32 = row.get(5)?;
                    let is_archived: i32 = row.get(6)?;
                    let created_at: i64 = row.get(7)?;
                    let updated_at: i64 = row.get(8)?;
                    Ok((id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at))
                })?;
                rows.filter_map(|r| r.ok())
                    .map(|(id, entity_type, user_id, name, slug, is_private, is_archived, created_at, updated_at)| {
                        let entity = Entity {
                            entity_type: EntityType::new(entity_type),
                            user_id: user_id.map(UserId::from_string),
                            name,
                            slug,
                            is_private: is_private != 0,
                            is_archived: is_archived != 0,
                        };
                        stored_editable(EntityId::from_string(id), entity, created_at, updated_at)
                    })
                    .collect()
            }
        };

        Ok(entities)
    }

    async fn update_entity(&self, id: &EntityId, entity: &Entity) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();

        conn.execute(
            "UPDATE entities SET name = ?1, slug = ?2, is_private = ?3, is_archived = ?4, updated_at = ?5
             WHERE id = ?6",
            params![
                entity.name,
                entity.slug,
                entity.is_private as i32,
                entity.is_archived as i32,
                now,
                id.as_str()
            ],
        )?;

        Ok(())
    }

    async fn archive_entity(&self, id: &EntityId) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();

        conn.execute(
            "UPDATE entities SET is_archived = 1, updated_at = ?1 WHERE id = ?2",
            params![now, id.as_str()],
        )?;

        Ok(())
    }

    async fn delete_entity(&self, id: &EntityId) -> Result<()> {
        let conn = self.conn().lock().unwrap();

        // Delete relations first (both directions)
        conn.execute(
            "DELETE FROM entity_relations WHERE from_id = ?1 OR to_id = ?1",
            params![id.as_str()],
        )?;

        // Delete entity
        conn.execute("DELETE FROM entities WHERE id = ?1", params![id.as_str()])?;

        Ok(())
    }

    // ========================================================================
    // Relations
    // ========================================================================

    async fn add_relation(
        &self,
        from_id: &EntityId,
        to_id: &EntityId,
        relation: RelationType,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        let metadata_json = metadata.map(|m| m.to_string());

        conn.execute(
            "INSERT OR REPLACE INTO entity_relations (from_id, to_id, relation, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                from_id.as_str(),
                to_id.as_str(),
                relation.as_str(),
                metadata_json,
                now
            ],
        )?;

        Ok(())
    }

    async fn get_relations_from(
        &self,
        id: &EntityId,
        relation_type: Option<&RelationType>,
    ) -> Result<Vec<(EntityId, EntityRelation)>> {
        let conn = self.conn().lock().unwrap();

        let relation_type_str = relation_type.map(|rt| rt.as_str().to_string());

        let results: Vec<(EntityId, EntityRelation)> = match &relation_type_str {
            Some(rt_str) => {
                let mut stmt = conn.prepare(
                    "SELECT to_id, relation, metadata, created_at
                     FROM entity_relations
                     WHERE from_id = ?1 AND relation = ?2
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![id.as_str(), rt_str], |row| {
                    let to_id: String = row.get(0)?;
                    let relation: String = row.get(1)?;
                    let metadata: Option<String> = row.get(2)?;
                    let created_at: i64 = row.get(3)?;
                    Ok((to_id, relation, metadata, created_at))
                })?;
                rows.filter_map(|r| r.ok())
                    .map(|(to_id, relation, metadata, created_at)| {
                        let entity_relation = EntityRelation {
                            relation: RelationType::new(relation),
                            metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
                            created_at,
                        };
                        (EntityId::from_string(to_id), entity_relation)
                    })
                    .collect()
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT to_id, relation, metadata, created_at
                     FROM entity_relations
                     WHERE from_id = ?1
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![id.as_str()], |row| {
                    let to_id: String = row.get(0)?;
                    let relation: String = row.get(1)?;
                    let metadata: Option<String> = row.get(2)?;
                    let created_at: i64 = row.get(3)?;
                    Ok((to_id, relation, metadata, created_at))
                })?;
                rows.filter_map(|r| r.ok())
                    .map(|(to_id, relation, metadata, created_at)| {
                        let entity_relation = EntityRelation {
                            relation: RelationType::new(relation),
                            metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
                            created_at,
                        };
                        (EntityId::from_string(to_id), entity_relation)
                    })
                    .collect()
            }
        };

        Ok(results)
    }

    async fn get_relations_to(
        &self,
        id: &EntityId,
        relation_type: Option<&RelationType>,
    ) -> Result<Vec<(EntityId, EntityRelation)>> {
        let conn = self.conn().lock().unwrap();

        let relation_type_str = relation_type.map(|rt| rt.as_str().to_string());

        let results: Vec<(EntityId, EntityRelation)> = match &relation_type_str {
            Some(rt_str) => {
                let mut stmt = conn.prepare(
                    "SELECT from_id, relation, metadata, created_at
                     FROM entity_relations
                     WHERE to_id = ?1 AND relation = ?2
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![id.as_str(), rt_str], |row| {
                    let from_id: String = row.get(0)?;
                    let relation: String = row.get(1)?;
                    let metadata: Option<String> = row.get(2)?;
                    let created_at: i64 = row.get(3)?;
                    Ok((from_id, relation, metadata, created_at))
                })?;
                rows.filter_map(|r| r.ok())
                    .map(|(from_id, relation, metadata, created_at)| {
                        let entity_relation = EntityRelation {
                            relation: RelationType::new(relation),
                            metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
                            created_at,
                        };
                        (EntityId::from_string(from_id), entity_relation)
                    })
                    .collect()
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT from_id, relation, metadata, created_at
                     FROM entity_relations
                     WHERE to_id = ?1
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![id.as_str()], |row| {
                    let from_id: String = row.get(0)?;
                    let relation: String = row.get(1)?;
                    let metadata: Option<String> = row.get(2)?;
                    let created_at: i64 = row.get(3)?;
                    Ok((from_id, relation, metadata, created_at))
                })?;
                rows.filter_map(|r| r.ok())
                    .map(|(from_id, relation, metadata, created_at)| {
                        let entity_relation = EntityRelation {
                            relation: RelationType::new(relation),
                            metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
                            created_at,
                        };
                        (EntityId::from_string(from_id), entity_relation)
                    })
                    .collect()
            }
        };

        Ok(results)
    }

    async fn remove_relation(
        &self,
        from_id: &EntityId,
        to_id: &EntityId,
        relation: &RelationType,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();

        conn.execute(
            "DELETE FROM entity_relations WHERE from_id = ?1 AND to_id = ?2 AND relation = ?3",
            params![from_id.as_str(), to_id.as_str(), relation.as_str()],
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::implementations::sqlite::SqliteStore;
    use crate::storage::traits::EntityStore;

    #[tokio::test]
    async fn test_create_and_get_entity() {
        let store = SqliteStore::in_memory().unwrap();
        let user_id = UserId::new();

        // Create user first (required for FK)
        {
            let conn = store.conn().lock().unwrap();
            conn.execute(
                "INSERT INTO users (id, email, created_at) VALUES (?1, ?2, ?3)",
                params![user_id.as_str(), "test@example.com", 1000],
            ).unwrap();
        }

        // Create entity
        let entity_id = store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();

        // Get entity
        let entity = store.get_entity(&entity_id).await.unwrap().unwrap();
        assert_eq!(entity.entity_type.as_str(), "conversation");
        assert_eq!(entity.user_id.as_ref().map(|u| u.as_str()), Some(user_id.as_str()));
        assert!(entity.is_private); // Default
        assert!(!entity.is_archived); // Default
    }

    #[tokio::test]
    async fn test_update_entity() {
        let store = SqliteStore::in_memory().unwrap();

        let entity_id = store
            .create_entity(EntityType::document(), None)
            .await
            .unwrap();

        // Update entity
        let mut entity = store.get_entity(&entity_id).await.unwrap().unwrap();
        entity.name = Some("My Document".to_string());
        entity.slug = Some("my-doc".to_string());
        entity.is_private = false;
        store.update_entity(&entity_id, &entity).await.unwrap();

        // Verify update
        let updated = store.get_entity(&entity_id).await.unwrap().unwrap();
        assert_eq!(updated.name.as_deref(), Some("My Document"));
        assert_eq!(updated.slug.as_deref(), Some("my-doc"));
        assert!(!updated.is_private);
    }

    #[tokio::test]
    async fn test_get_entity_by_slug() {
        let store = SqliteStore::in_memory().unwrap();

        let entity_id = store
            .create_entity(EntityType::conversation(), None)
            .await
            .unwrap();

        // Set slug
        let mut entity = store.get_entity(&entity_id).await.unwrap().unwrap();
        entity.slug = Some("my-conversation".to_string());
        store.update_entity(&entity_id, &entity).await.unwrap();

        // Get by slug
        let found = store
            .get_entity_by_slug("my-conversation")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, entity_id);
    }

    #[tokio::test]
    async fn test_list_entities() {
        let store = SqliteStore::in_memory().unwrap();
        let user_id = UserId::new();

        // Create user
        {
            let conn = store.conn().lock().unwrap();
            conn.execute(
                "INSERT INTO users (id, email, created_at) VALUES (?1, ?2, ?3)",
                params![user_id.as_str(), "test@example.com", 1000],
            ).unwrap();
        }

        // Create entities
        store.create_entity(EntityType::conversation(), Some(&user_id)).await.unwrap();
        store.create_entity(EntityType::conversation(), Some(&user_id)).await.unwrap();
        store.create_entity(EntityType::document(), Some(&user_id)).await.unwrap();

        // List all
        let all = store.list_entities(&user_id, None).await.unwrap();
        assert_eq!(all.len(), 3);

        // List by type
        let conversations = store
            .list_entities(&user_id, Some(&EntityType::conversation()))
            .await
            .unwrap();
        assert_eq!(conversations.len(), 2);

        let documents = store
            .list_entities(&user_id, Some(&EntityType::document()))
            .await
            .unwrap();
        assert_eq!(documents.len(), 1);
    }

    #[tokio::test]
    async fn test_archive_entity() {
        let store = SqliteStore::in_memory().unwrap();
        let user_id = UserId::new();

        // Create user
        {
            let conn = store.conn().lock().unwrap();
            conn.execute(
                "INSERT INTO users (id, email, created_at) VALUES (?1, ?2, ?3)",
                params![user_id.as_str(), "test@example.com", 1000],
            ).unwrap();
        }

        let entity_id = store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();

        // Archive
        store.archive_entity(&entity_id).await.unwrap();

        // Entity is archived
        let entity = store.get_entity(&entity_id).await.unwrap().unwrap();
        assert!(entity.is_archived);

        // Not in default list
        let list = store.list_entities(&user_id, None).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_delete_entity() {
        let store = SqliteStore::in_memory().unwrap();

        let entity_id = store
            .create_entity(EntityType::asset(), None)
            .await
            .unwrap();

        // Delete
        store.delete_entity(&entity_id).await.unwrap();

        // Gone
        let entity = store.get_entity(&entity_id).await.unwrap();
        assert!(entity.is_none());
    }

    #[tokio::test]
    async fn test_add_and_get_relations() {
        let store = SqliteStore::in_memory().unwrap();

        // Create two entities
        let entity1 = store.create_entity(EntityType::conversation(), None).await.unwrap();
        let entity2 = store.create_entity(EntityType::conversation(), None).await.unwrap();

        // Add relation with metadata
        let metadata = serde_json::json!({"at_turn_id": "turn-123"});
        store
            .add_relation(&entity2, &entity1, RelationType::forked_from(), Some(metadata.clone()))
            .await
            .unwrap();

        // Get relations from entity2
        let from_relations = store
            .get_relations_from(&entity2, None)
            .await
            .unwrap();
        assert_eq!(from_relations.len(), 1);
        assert_eq!(from_relations[0].0, entity1);
        assert_eq!(from_relations[0].1.relation.as_str(), "forked_from");
        assert_eq!(from_relations[0].1.metadata, Some(metadata.clone()));

        // Get relations to entity1 (backlinks)
        let to_relations = store
            .get_relations_to(&entity1, None)
            .await
            .unwrap();
        assert_eq!(to_relations.len(), 1);
        assert_eq!(to_relations[0].0, entity2);

        // Filter by relation type
        let forked = store
            .get_relations_from(&entity2, Some(&RelationType::forked_from()))
            .await
            .unwrap();
        assert_eq!(forked.len(), 1);

        let references = store
            .get_relations_from(&entity2, Some(&RelationType::references()))
            .await
            .unwrap();
        assert!(references.is_empty());
    }

    #[tokio::test]
    async fn test_remove_relation() {
        let store = SqliteStore::in_memory().unwrap();

        let entity1 = store.create_entity(EntityType::document(), None).await.unwrap();
        let entity2 = store.create_entity(EntityType::document(), None).await.unwrap();

        // Add relation
        store
            .add_relation(&entity1, &entity2, RelationType::references(), None)
            .await
            .unwrap();

        // Remove relation
        store
            .remove_relation(&entity1, &entity2, &RelationType::references())
            .await
            .unwrap();

        // Relation is gone
        let relations = store.get_relations_from(&entity1, None).await.unwrap();
        assert!(relations.is_empty());
    }

    #[tokio::test]
    async fn test_delete_entity_removes_relations() {
        let store = SqliteStore::in_memory().unwrap();

        let entity1 = store.create_entity(EntityType::conversation(), None).await.unwrap();
        let entity2 = store.create_entity(EntityType::conversation(), None).await.unwrap();

        // Add relation
        store
            .add_relation(&entity1, &entity2, RelationType::forked_from(), None)
            .await
            .unwrap();

        // Delete entity1
        store.delete_entity(&entity1).await.unwrap();

        // Relations involving entity1 are gone
        let to_relations = store.get_relations_to(&entity2, None).await.unwrap();
        assert!(to_relations.is_empty());
    }
}
