//! In-memory ReferenceStore implementation (stub)

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{EntityId, ReferenceId};
use crate::storage::traits::{ReferenceStore, StoredReference};
use crate::storage::types::RelationType;

/// In-memory reference store (stub implementation)
pub struct MemoryReferenceStore;

impl MemoryReferenceStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MemoryReferenceStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ReferenceStore for MemoryReferenceStore {
    async fn create_reference(
        &self,
        _from_entity_id: &EntityId,
        _to_entity_id: &EntityId,
        _relation_type: Option<&RelationType>,
        _context: Option<&str>,
    ) -> Result<ReferenceId> {
        unimplemented!("MemoryReferenceStore::create_reference")
    }

    async fn delete_reference(&self, _id: &ReferenceId) -> Result<bool> {
        unimplemented!("MemoryReferenceStore::delete_reference")
    }

    async fn delete_references_between(
        &self,
        _from_entity_id: &EntityId,
        _to_entity_id: &EntityId,
    ) -> Result<usize> {
        unimplemented!("MemoryReferenceStore::delete_references_between")
    }

    async fn get_outgoing(&self, _entity_id: &EntityId) -> Result<Vec<StoredReference>> {
        unimplemented!("MemoryReferenceStore::get_outgoing")
    }

    async fn get_outgoing_by_type(
        &self,
        _entity_id: &EntityId,
        _relation_type: &RelationType,
    ) -> Result<Vec<StoredReference>> {
        unimplemented!("MemoryReferenceStore::get_outgoing_by_type")
    }

    async fn get_backlinks(&self, _entity_id: &EntityId) -> Result<Vec<StoredReference>> {
        unimplemented!("MemoryReferenceStore::get_backlinks")
    }

    async fn get_backlinks_by_type(
        &self,
        _entity_id: &EntityId,
        _relation_type: &RelationType,
    ) -> Result<Vec<StoredReference>> {
        unimplemented!("MemoryReferenceStore::get_backlinks_by_type")
    }

    async fn reference_exists(
        &self,
        _from_entity_id: &EntityId,
        _to_entity_id: &EntityId,
        _relation_type: Option<&RelationType>,
    ) -> Result<bool> {
        unimplemented!("MemoryReferenceStore::reference_exists")
    }
}
