//! In-memory EntityStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::ids::{EntityId, UserId};
use crate::storage::traits::{EntityStore, StoredEntity};
use crate::storage::types::entity::{Entity, EntityRelation, EntityType, RelationType};
use crate::storage::types::stored_editable;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Storage entry for an entity
#[derive(Clone, Debug)]
struct EntityEntry {
    id: EntityId,
    entity_type: EntityType,
    user_id: Option<UserId>,
    name: Option<String>,
    slug: Option<String>,
    is_private: bool,
    is_archived: bool,
    created_at: i64,
    updated_at: i64,
}

impl EntityEntry {
    fn to_stored(&self) -> StoredEntity {
        let entity = Entity {
            entity_type: self.entity_type.clone(),
            user_id: self.user_id.clone(),
            name: self.name.clone(),
            slug: self.slug.clone(),
            is_private: self.is_private,
            is_archived: self.is_archived,
        };
        stored_editable(self.id.clone(), entity, self.created_at, self.updated_at)
    }
}

/// Relation key for indexing
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RelationKey {
    from_id: String,
    to_id: String,
    relation: String,
}

/// Storage entry for a relation
#[derive(Clone, Debug)]
struct RelationEntry {
    from_id: EntityId,
    to_id: EntityId,
    relation: RelationType,
    metadata: Option<serde_json::Value>,
    created_at: i64,
}

/// In-memory entity store for testing
#[derive(Debug, Default)]
pub struct MemoryEntityStore {
    entities: Mutex<HashMap<String, EntityEntry>>,
    slugs: Mutex<HashMap<String, String>>, // slug -> entity_id
    relations: Mutex<HashMap<RelationKey, RelationEntry>>,
}

impl MemoryEntityStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl EntityStore for MemoryEntityStore {
    async fn create_entity(
        &self,
        entity_type: EntityType,
        user_id: Option<&UserId>,
    ) -> Result<EntityId> {
        let id = EntityId::new();
        let now = now();
        let entry = EntityEntry {
            id: id.clone(),
            entity_type,
            user_id: user_id.cloned(),
            name: None,
            slug: None,
            is_private: true,
            is_archived: false,
            created_at: now,
            updated_at: now,
        };
        self.entities
            .lock()
            .unwrap()
            .insert(id.as_str().to_string(), entry);
        Ok(id)
    }

    async fn get_entity(&self, id: &EntityId) -> Result<Option<StoredEntity>> {
        let entities = self.entities.lock().unwrap();
        Ok(entities.get(id.as_str()).map(|e| e.to_stored()))
    }

    async fn get_entity_by_slug(&self, slug: &str) -> Result<Option<StoredEntity>> {
        let slugs = self.slugs.lock().unwrap();
        let entity_id = match slugs.get(slug) {
            Some(id) => id.clone(),
            None => return Ok(None),
        };
        drop(slugs);

        let entities = self.entities.lock().unwrap();
        Ok(entities.get(&entity_id).map(|e| e.to_stored()))
    }

    async fn list_entities(
        &self,
        user_id: &UserId,
        entity_type: Option<&EntityType>,
    ) -> Result<Vec<StoredEntity>> {
        let entities = self.entities.lock().unwrap();
        let mut result: Vec<_> = entities
            .values()
            .filter(|e| e.user_id.as_ref() == Some(user_id))
            .filter(|e| !e.is_archived)
            .filter(|e| entity_type.map_or(true, |t| &e.entity_type == t))
            .map(|e| e.to_stored())
            .collect();
        result.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(result)
    }

    async fn update_entity(&self, id: &EntityId, entity: &Entity) -> Result<()> {
        let mut entities = self.entities.lock().unwrap();
        if let Some(entry) = entities.get_mut(id.as_str()) {
            // Update slug index
            if entry.slug != entity.slug {
                let mut slugs = self.slugs.lock().unwrap();
                if let Some(old_slug) = &entry.slug {
                    slugs.remove(old_slug);
                }
                if let Some(new_slug) = &entity.slug {
                    slugs.insert(new_slug.clone(), id.as_str().to_string());
                }
            }

            entry.name = entity.name.clone();
            entry.slug = entity.slug.clone();
            entry.is_private = entity.is_private;
            entry.is_archived = entity.is_archived;
            entry.updated_at = now();
        }
        Ok(())
    }

    async fn archive_entity(&self, id: &EntityId) -> Result<()> {
        let mut entities = self.entities.lock().unwrap();
        if let Some(entry) = entities.get_mut(id.as_str()) {
            entry.is_archived = true;
            entry.updated_at = now();
        }
        Ok(())
    }

    async fn delete_entity(&self, id: &EntityId) -> Result<()> {
        // Remove slug index
        {
            let entities = self.entities.lock().unwrap();
            if let Some(entry) = entities.get(id.as_str()) {
                if let Some(slug) = &entry.slug {
                    self.slugs.lock().unwrap().remove(slug);
                }
            }
        }

        // Remove relations
        {
            let mut relations = self.relations.lock().unwrap();
            relations.retain(|k, _| k.from_id != id.as_str() && k.to_id != id.as_str());
        }

        // Remove entity
        self.entities.lock().unwrap().remove(id.as_str());
        Ok(())
    }

    async fn add_relation(
        &self,
        from_id: &EntityId,
        to_id: &EntityId,
        relation: RelationType,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let key = RelationKey {
            from_id: from_id.as_str().to_string(),
            to_id: to_id.as_str().to_string(),
            relation: relation.as_str().to_string(),
        };
        let entry = RelationEntry {
            from_id: from_id.clone(),
            to_id: to_id.clone(),
            relation,
            metadata,
            created_at: now(),
        };
        self.relations.lock().unwrap().insert(key, entry);
        Ok(())
    }

    async fn get_relations_from(
        &self,
        id: &EntityId,
        relation_type: Option<&RelationType>,
    ) -> Result<Vec<(EntityId, EntityRelation)>> {
        let relations = self.relations.lock().unwrap();
        let mut result: Vec<_> = relations
            .values()
            .filter(|e| e.from_id == *id)
            .filter(|e| relation_type.map_or(true, |t| &e.relation == t))
            .map(|e| {
                (
                    e.to_id.clone(),
                    EntityRelation {
                        relation: e.relation.clone(),
                        metadata: e.metadata.clone(),
                        created_at: e.created_at,
                    },
                )
            })
            .collect();
        result.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));
        Ok(result)
    }

    async fn get_relations_to(
        &self,
        id: &EntityId,
        relation_type: Option<&RelationType>,
    ) -> Result<Vec<(EntityId, EntityRelation)>> {
        let relations = self.relations.lock().unwrap();
        let mut result: Vec<_> = relations
            .values()
            .filter(|e| e.to_id == *id)
            .filter(|e| relation_type.map_or(true, |t| &e.relation == t))
            .map(|e| {
                (
                    e.from_id.clone(),
                    EntityRelation {
                        relation: e.relation.clone(),
                        metadata: e.metadata.clone(),
                        created_at: e.created_at,
                    },
                )
            })
            .collect();
        result.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));
        Ok(result)
    }

    async fn remove_relation(
        &self,
        from_id: &EntityId,
        to_id: &EntityId,
        relation: &RelationType,
    ) -> Result<()> {
        let key = RelationKey {
            from_id: from_id.as_str().to_string(),
            to_id: to_id.as_str().to_string(),
            relation: relation.as_str().to_string(),
        };
        self.relations.lock().unwrap().remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get_entity() {
        let store = MemoryEntityStore::new();
        let user_id = UserId::new();

        let entity_id = store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();

        let entity = store.get_entity(&entity_id).await.unwrap().unwrap();
        assert_eq!(entity.entity_type.as_str(), "conversation");
        assert!(entity.is_private);
    }

    #[tokio::test]
    async fn test_list_entities_by_type() {
        let store = MemoryEntityStore::new();
        let user_id = UserId::new();

        store.create_entity(EntityType::conversation(), Some(&user_id)).await.unwrap();
        store.create_entity(EntityType::conversation(), Some(&user_id)).await.unwrap();
        store.create_entity(EntityType::document(), Some(&user_id)).await.unwrap();

        let conversations = store
            .list_entities(&user_id, Some(&EntityType::conversation()))
            .await
            .unwrap();
        assert_eq!(conversations.len(), 2);

        let all = store.list_entities(&user_id, None).await.unwrap();
        assert_eq!(all.len(), 3);
    }
}
