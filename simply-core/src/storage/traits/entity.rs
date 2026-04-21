//! EntityStore trait for the addressable layer
//!
//! Provides unified CRUD operations for all entity types (conversations, documents,
//! tabs, notes, directories, labels) and management of relationships between entities.

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{AssetId, EntityId, UserId};
use crate::storage::types::entity::{Entity, EntityRangeQuery, EntityRelation, EntityType, RelationType};
use crate::storage::types::StoredEditable;

/// Stored representation of an Entity (mutable - can be renamed, archived, etc.)
pub type StoredEntity = StoredEditable<EntityId, Entity>;

/// Trait for entity storage operations
///
/// Provides unified identity, naming, and relationships for all addressable things.
/// Text content lives in `content_blocks` and is linked via `entities.content_block_id`.
/// Image/asset references are tracked in `entity_assets` for blob GC joins.
#[async_trait]
pub trait EntityStore: Send + Sync {
    // ========================================================================
    // Entity CRUD
    // ========================================================================

    /// Create a new entity
    ///
    /// Returns the new entity ID. The entity starts with default values
    /// (private, no name/content/origin).
    async fn create_entity(
        &self,
        entity_type: EntityType,
        user_id: Option<&UserId>,
    ) -> Result<EntityId>;

    /// Get an entity by ID
    async fn get_entity(&self, id: &EntityId) -> Result<Option<StoredEntity>>;

    /// Look up an entity by exact origin string (e.g. `"google_drive:gdoc-abc"`).
    /// Used by import skills to find the existing imported version of an
    /// external resource.
    async fn get_entity_by_origin(
        &self,
        user_id: &UserId,
        origin: &str,
    ) -> Result<Option<StoredEntity>>;

    /// List entities for a user, optionally filtered by type
    async fn list_entities(
        &self,
        user_id: &UserId,
        entity_type: Option<&EntityType>,
    ) -> Result<Vec<StoredEntity>>;

    /// List entities whose `entity_type` matches the given prefix (e.g. `"document::"`
    /// for all document-kinds). Useful for UI listings that span several subtypes.
    async fn list_entities_by_type_prefix(
        &self,
        user_id: &UserId,
        prefix: &str,
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
    /// Updates name, is_private, content_block_id, origin, metadata.
    /// Entity type and user_id cannot be changed here — use
    /// [`set_entity_type`](Self::set_entity_type) for type changes.
    async fn update_entity(&self, id: &EntityId, entity: &Entity) -> Result<()>;

    /// Change an entity's `entity_type` in place.
    ///
    /// Carved out of [`update_entity`](Self::update_entity) because it's a
    /// deliberately loud operation: conversion between kinds (note → todo,
    /// flat → tabbed) changes how UIs render and how RAG treats the entity.
    /// The entity id is preserved so inbound relations survive.
    async fn set_entity_type(&self, id: &EntityId, new_type: EntityType) -> Result<()>;

    /// Delete an entity permanently
    ///
    /// Also removes all relations involving this entity and the asset mappings
    /// from `entity_assets`. Does NOT cascade to other entities - the orchestrator
    /// must walk `structure::contained_in` children first if a tree should go away.
    async fn delete_entity(&self, id: &EntityId) -> Result<()>;

    // ========================================================================
    // Relations
    // ========================================================================

    /// Add a relation between entities
    ///
    /// Relations are directional: from_id relates to to_id. `position` gives
    /// sibling ordering for ordered relations (e.g. tab_index); `metadata`
    /// carries relation-specific extras.
    async fn add_relation(
        &self,
        from_id: &EntityId,
        to_id: &EntityId,
        relation: RelationType,
        position: Option<i64>,
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

    /// Get relations to an entity, ordered by `position` ascending (NULLs last).
    /// Used to list ordered children such as the tabs inside a tabbed document.
    async fn list_relations_to_ordered(
        &self,
        id: &EntityId,
        relation_type: &RelationType,
    ) -> Result<Vec<(EntityId, EntityRelation)>>;

    /// Get relations from an entity, ordered by `position` ascending (NULLs last).
    async fn list_relations_from_ordered(
        &self,
        id: &EntityId,
        relation_type: &RelationType,
    ) -> Result<Vec<(EntityId, EntityRelation)>>;

    /// Remove a specific relation
    async fn remove_relation(
        &self,
        from_id: &EntityId,
        to_id: &EntityId,
        relation: &RelationType,
    ) -> Result<()>;

    // ========================================================================
    // Entity ↔ Asset mappings (for blob GC)
    // ========================================================================

    /// Replace the full set of assets referenced by an entity.
    /// Used when a tab / flat doc's markdown embeds a new set of images.
    async fn set_entity_assets(&self, entity_id: &EntityId, asset_ids: &[AssetId]) -> Result<()>;

    /// Get all assets referenced by an entity.
    async fn get_entity_assets(&self, entity_id: &EntityId) -> Result<Vec<AssetId>>;

    /// Get all entities that reference a given asset. Used by blob GC to check
    /// if an asset is still reachable before deleting.
    async fn entities_referencing_asset(&self, asset_id: &AssetId) -> Result<Vec<EntityId>>;
}
