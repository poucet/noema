//! ReferenceStore trait for cross-references between entities

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{EntityId, ReferenceId};
use crate::storage::types::{Reference, RelationType, Stored};

/// Stored representation of a Reference (immutable once created)
pub type StoredReference = Stored<ReferenceId, Reference>;

/// Trait for cross-reference storage operations
///
/// Cross-references link entities to each other with optional relation types
/// and context. Backlinks are computed by querying references where
/// `to_entity_id` matches the target.
#[async_trait]
pub trait ReferenceStore: Send + Sync {
    /// Create a new reference from one entity to another
    ///
    /// Returns the new reference ID. If a reference with the same
    /// (from, to, relation_type) already exists, returns an error.
    async fn create_reference(
        &self,
        from_entity_id: &EntityId,
        to_entity_id: &EntityId,
        relation_type: Option<&RelationType>,
        context: Option<&str>,
    ) -> Result<ReferenceId>;

    /// Delete a reference by ID
    ///
    /// Returns true if the reference existed and was deleted.
    async fn delete_reference(&self, id: &ReferenceId) -> Result<bool>;

    /// Delete all references from a specific entity to another
    ///
    /// Useful when removing a link regardless of relation type.
    /// Returns the number of references deleted.
    async fn delete_references_between(
        &self,
        from_entity_id: &EntityId,
        to_entity_id: &EntityId,
    ) -> Result<usize>;

    /// Get all outgoing references from an entity
    ///
    /// Returns references where `from_entity_id` matches the given ID.
    async fn get_outgoing(&self, entity_id: &EntityId) -> Result<Vec<StoredReference>>;

    /// Get all outgoing references of a specific type from an entity
    async fn get_outgoing_by_type(
        &self,
        entity_id: &EntityId,
        relation_type: &RelationType,
    ) -> Result<Vec<StoredReference>>;

    /// Get all backlinks (incoming references) to an entity
    ///
    /// Returns references where `to_entity_id` matches the given ID.
    async fn get_backlinks(&self, entity_id: &EntityId) -> Result<Vec<StoredReference>>;

    /// Get all backlinks of a specific type to an entity
    async fn get_backlinks_by_type(
        &self,
        entity_id: &EntityId,
        relation_type: &RelationType,
    ) -> Result<Vec<StoredReference>>;

    /// Check if a reference exists between two entities
    async fn reference_exists(
        &self,
        from_entity_id: &EntityId,
        to_entity_id: &EntityId,
        relation_type: Option<&RelationType>,
    ) -> Result<bool>;
}
