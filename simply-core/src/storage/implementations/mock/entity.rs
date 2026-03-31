//! Mock entity store for testing

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{EntityId, UserId};
use crate::storage::traits::{EntityStore, StoredEntity};
use crate::storage::types::entity::{EntityRangeQuery, EntityRelation, EntityType, RelationType};

/// Mock entity store that returns unimplemented for all operations
pub struct MockEntityStore;

#[async_trait]
impl EntityStore for MockEntityStore {
    async fn create_entity(&self, _: EntityType, _: Option<&UserId>) -> Result<EntityId> {
        unimplemented!()
    }
    async fn get_entity(&self, _: &EntityId) -> Result<Option<StoredEntity>> {
        unimplemented!()
    }
    async fn get_entity_by_slug(&self, _: &str) -> Result<Option<StoredEntity>> {
        unimplemented!()
    }
    async fn list_entities(&self, _: &UserId, _: Option<&EntityType>) -> Result<Vec<StoredEntity>> {
        unimplemented!()
    }
    async fn list_entities_in_range(&self, _: &UserId, _: &EntityRangeQuery) -> Result<Vec<StoredEntity>> {
        unimplemented!()
    }
    async fn update_entity(&self, _: &EntityId, _: &crate::storage::types::Entity) -> Result<()> {
        unimplemented!()
    }
    async fn archive_entity(&self, _: &EntityId) -> Result<()> {
        unimplemented!()
    }
    async fn delete_entity(&self, _: &EntityId) -> Result<()> {
        unimplemented!()
    }
    async fn add_relation(
        &self,
        _: &EntityId,
        _: &EntityId,
        _: RelationType,
        _: Option<serde_json::Value>,
    ) -> Result<()> {
        unimplemented!()
    }
    async fn get_relations_from(
        &self,
        _: &EntityId,
        _: Option<&RelationType>,
    ) -> Result<Vec<(EntityId, EntityRelation)>> {
        unimplemented!()
    }
    async fn get_relations_to(
        &self,
        _: &EntityId,
        _: Option<&RelationType>,
    ) -> Result<Vec<(EntityId, EntityRelation)>> {
        unimplemented!()
    }
    async fn remove_relation(&self, _: &EntityId, _: &EntityId, _: &RelationType) -> Result<()> {
        unimplemented!()
    }
}
