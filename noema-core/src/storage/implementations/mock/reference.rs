//! Mock ReferenceStore implementation

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{EntityId, ReferenceId};
use crate::storage::traits::{ReferenceStore, StoredReference};
use crate::storage::types::RelationType;

/// Mock reference store (returns unimplemented for all methods)
pub struct MockReferenceStore;

#[async_trait]
impl ReferenceStore for MockReferenceStore {
    async fn create_reference(
        &self,
        _from_entity_id: &EntityId,
        _to_entity_id: &EntityId,
        _relation_type: Option<&RelationType>,
        _context: Option<&str>,
    ) -> Result<ReferenceId> {
        unimplemented!()
    }

    async fn delete_reference(&self, _id: &ReferenceId) -> Result<bool> {
        unimplemented!()
    }

    async fn delete_references_between(
        &self,
        _from_entity_id: &EntityId,
        _to_entity_id: &EntityId,
    ) -> Result<usize> {
        unimplemented!()
    }

    async fn get_outgoing(&self, _entity_id: &EntityId) -> Result<Vec<StoredReference>> {
        unimplemented!()
    }

    async fn get_outgoing_by_type(
        &self,
        _entity_id: &EntityId,
        _relation_type: &RelationType,
    ) -> Result<Vec<StoredReference>> {
        unimplemented!()
    }

    async fn get_backlinks(&self, _entity_id: &EntityId) -> Result<Vec<StoredReference>> {
        unimplemented!()
    }

    async fn get_backlinks_by_type(
        &self,
        _entity_id: &EntityId,
        _relation_type: &RelationType,
    ) -> Result<Vec<StoredReference>> {
        unimplemented!()
    }

    async fn reference_exists(
        &self,
        _from_entity_id: &EntityId,
        _to_entity_id: &EntityId,
        _relation_type: Option<&RelationType>,
    ) -> Result<bool> {
        unimplemented!()
    }
}
