//! EntityStore trait for the addressable layer
//!
//! Provides unified CRUD operations for all entity types (conversations, documents, assets)
//! and management of relationships between entities.

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{EntityId, UserId};
use crate::storage::types::entity::{Entity, EntityRangeQuery, EntityRelation, EntityType, RelationType};
use crate::storage::types::StoredEditable;

/// Stored representation of an Entity (mutable - can be renamed, archived, etc.)
pub type StoredEntity = StoredEditable<EntityId, Entity>;

/// Trait for entity storage operations
///
/// Provides unified identity, naming, and relationships for all addressable things.
/// Domain-specific structure (view selections, document revisions) lives in separate
/// stores that use the entity ID as their primary key.
#[async_trait]
pub trait EntityStore: Send + Sync {
    // ========================================================================
    // Entity CRUD
    // ========================================================================

    /// Create a new entity
    ///
    /// Returns the new entity ID. The entity starts with default values
    /// (private, not archived, no name/slug).
    async fn create_entity(
        &self,
        entity_type: EntityType,
        user_id: Option<&UserId>,
    ) -> Result<EntityId>;

    /// Get an entity by ID
    async fn get_entity(&self, id: &EntityId) -> Result<Option<StoredEntity>>;

    /// Get an entity by slug (@mention)
    async fn get_entity_by_slug(&self, slug: &str) -> Result<Option<StoredEntity>>;

    /// List entities for a user, optionally filtered by type
    async fn list_entities(
        &self,
        user_id: &UserId,
        entity_type: Option<&EntityType>,
    ) -> Result<Vec<StoredEntity>>;

    /// List entities updated within a time range
    ///
    /// Returns entities ordered by `updated_at` descending (most recent first).
    /// Excludes archived entities.
    async fn list_entities_in_range(
        &self,
        user_id: &UserId,
        query: &EntityRangeQuery,
    ) -> Result<Vec<StoredEntity>>;

    /// Update an entity's mutable fields
    ///
    /// Updates name, slug, is_private, is_archived. Entity type cannot be changed.
    async fn update_entity(&self, id: &EntityId, entity: &Entity) -> Result<()>;

    /// Archive an entity (soft delete - hidden from default views)
    async fn archive_entity(&self, id: &EntityId) -> Result<()>;

    /// Delete an entity permanently
    ///
    /// Also removes all relations involving this entity.
    /// Does NOT cascade to domain-specific data - caller must handle that.
    async fn delete_entity(&self, id: &EntityId) -> Result<()>;

    // ========================================================================
    // Relations
    // ========================================================================

    /// Add a relation between entities
    ///
    /// Relations are directional: from_id relates to to_id.
    /// For symmetric relations, add both directions.
    async fn add_relation(
        &self,
        from_id: &EntityId,
        to_id: &EntityId,
        relation: RelationType,
        metadata: Option<serde_json::Value>,
    ) -> Result<()>;

    /// Get relations from an entity
    ///
    /// If relation_type is Some, filters to that type only.
    async fn get_relations_from(
        &self,
        id: &EntityId,
        relation_type: Option<&RelationType>,
    ) -> Result<Vec<(EntityId, EntityRelation)>>;

    /// Get relations to an entity (backlinks)
    ///
    /// If relation_type is Some, filters to that type only.
    async fn get_relations_to(
        &self,
        id: &EntityId,
        relation_type: Option<&RelationType>,
    ) -> Result<Vec<(EntityId, EntityRelation)>>;

    /// Remove a specific relation
    async fn remove_relation(
        &self,
        from_id: &EntityId,
        to_id: &EntityId,
        relation: &RelationType,
    ) -> Result<()>;
}
