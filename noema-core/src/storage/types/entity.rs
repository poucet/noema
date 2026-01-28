//! Entity types for the addressable layer
//!
//! All addressable things (conversations, documents, assets) are entities with:
//! - Unified identity (id, type, name, slug)
//! - Consistent metadata (user, privacy, archive status)
//! - Relationships via entity_relations table

use serde::{Deserialize, Serialize};

use crate::storage::ids::UserId;

// ============================================================================
// EntityType
// ============================================================================

/// Type of entity in the addressable layer
///
/// Stored as a string for extensibility - new entity types can be added
/// without code changes. The frontend dispatches on this value to determine
/// how to render and what domain-specific data to fetch.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityType(String);

impl EntityType {
    /// Create a new entity type
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the string value
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Conversation entity type
    pub fn conversation() -> Self {
        Self::new("conversation")
    }

    /// Document entity type (with revision history)
    pub fn document() -> Self {
        Self::new("document")
    }

    /// Asset entity type (binary: image, audio, PDF)
    pub fn asset() -> Self {
        Self::new("asset")
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for EntityType {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for EntityType {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ============================================================================
// Entity
// ============================================================================

/// Core entity data for the addressable layer
///
/// Entities provide unified identity, naming, and metadata for all
/// addressable things. The specific structure (view selections, document
/// revisions, etc.) is stored in domain-specific tables that reference
/// the entity ID.
///
/// Use with `StoredEditable<EntityId, Entity>` for the full stored representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    /// Type of entity (extensible string, e.g., "conversation", "document", "asset")
    pub entity_type: EntityType,
    /// Owning user (optional for shared entities)
    pub user_id: Option<UserId>,
    /// Human-readable display name
    pub name: Option<String>,
    /// Unique slug for @mentions (user-assigned, optional)
    pub slug: Option<String>,
    /// Whether content is private (local-only, never sent to cloud)
    pub is_private: bool,
    /// Whether entity is archived (hidden from default views)
    pub is_archived: bool,
    /// Type-specific metadata as JSON
    /// For conversations: {"main_view_id": "view-123"}
    /// For documents: {"document_id": "doc-456"}
    /// For assets: {"asset_id": "asset-789"}
    pub metadata: Option<serde_json::Value>,
}

impl Entity {
    /// Create a new entity of the given type
    pub fn new(entity_type: EntityType) -> Self {
        Self {
            entity_type,
            user_id: None,
            name: None,
            slug: None,
            is_private: true,
            is_archived: false,
            metadata: None,
        }
    }

    /// Set the owning user
    pub fn with_user(mut self, user_id: UserId) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set the display name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the @mention slug
    pub fn with_slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = Some(slug.into());
        self
    }

    /// Mark as public (content can be sent to cloud models)
    pub fn public(mut self) -> Self {
        self.is_private = false;
        self
    }

    /// Mark as archived
    pub fn archived(mut self) -> Self {
        self.is_archived = true;
        self
    }

    /// Set type-specific metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

// ============================================================================
// EntityRangeQuery
// ============================================================================

/// Query parameters for time-range entity search
///
/// Used with `EntityStore::list_entities_in_range`.
#[derive(Clone, Debug, Default)]
pub struct EntityRangeQuery {
    /// Start of time range (unix timestamp ms, inclusive)
    pub start: i64,
    /// End of time range (unix timestamp ms, inclusive)
    pub end: i64,
    /// Filter to specific entity types (None = all types)
    pub entity_types: Option<Vec<EntityType>>,
    /// Maximum number of results (None = no limit)
    pub limit: Option<u32>,
}

impl EntityRangeQuery {
    /// Create a new query for a time range
    pub fn new(start: i64, end: i64) -> Self {
        Self {
            start,
            end,
            entity_types: None,
            limit: None,
        }
    }

    /// Filter to specific entity types
    pub fn with_types(mut self, types: impl IntoIterator<Item = EntityType>) -> Self {
        self.entity_types = Some(types.into_iter().collect());
        self
    }

    /// Limit number of results
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Get entity types as slice (for query)
    pub fn types_slice(&self) -> Option<&[EntityType]> {
        self.entity_types.as_deref()
    }
}

// ============================================================================
// RelationType
// ============================================================================

/// Type of relationship between entities
///
/// Stored as a string for extensibility - new relation types can be added
/// without code changes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelationType(String);

impl RelationType {
    /// Create a new relation type
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the string value
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Conversation forked from another conversation
    pub fn forked_from() -> Self {
        Self::new("forked_from")
    }

    /// Entity references another entity
    pub fn references() -> Self {
        Self::new("references")
    }

    /// Document derived from another document
    pub fn derived_from() -> Self {
        Self::new("derived_from")
    }

    /// Entities manually grouped together
    pub fn grouped_with() -> Self {
        Self::new("grouped_with")
    }

    /// Subconversation spawned from parent conversation
    /// Metadata: {"at_turn_id": "...", "at_span_id": "..."}
    pub fn spawned_from() -> Self {
        Self::new("spawned_from")
    }
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for RelationType {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for RelationType {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ============================================================================
// EntityRelation
// ============================================================================

/// A relationship between two entities
///
/// Relations are directional: from_id relates to to_id.
/// For symmetric relations (grouped_with), both directions should be stored.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityRelation {
    /// Type of relationship
    pub relation: RelationType,
    /// Optional JSON metadata (e.g., {at_turn_id: "..."} for forks)
    pub metadata: Option<serde_json::Value>,
    /// When the relation was created (unix timestamp ms)
    pub created_at: i64,
}

impl EntityRelation {
    /// Create a new relation
    pub fn new(relation: RelationType) -> Self {
        Self {
            relation,
            metadata: None,
            created_at: 0, // Set by store
        }
    }

    /// Add metadata to the relation
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_type_wellknown() {
        assert_eq!(EntityType::conversation().as_str(), "conversation");
        assert_eq!(EntityType::document().as_str(), "document");
        assert_eq!(EntityType::asset().as_str(), "asset");
    }

    #[test]
    fn test_entity_type_custom() {
        let custom = EntityType::new("my_plugin_type");
        assert_eq!(custom.as_str(), "my_plugin_type");
    }

    #[test]
    fn test_relation_type_wellknown() {
        assert_eq!(RelationType::forked_from().as_str(), "forked_from");
        assert_eq!(RelationType::references().as_str(), "references");
        assert_eq!(RelationType::derived_from().as_str(), "derived_from");
        assert_eq!(RelationType::grouped_with().as_str(), "grouped_with");
        assert_eq!(RelationType::spawned_from().as_str(), "spawned_from");
    }

    #[test]
    fn test_entity_builder() {
        let entity = Entity::new(EntityType::conversation())
            .with_user(UserId::from_string("user-1"))
            .with_name("My Conversation")
            .with_slug("my-convo")
            .public();

        assert_eq!(entity.entity_type.as_str(), "conversation");
        assert_eq!(entity.user_id.as_ref().map(|u| u.as_str()), Some("user-1"));
        assert_eq!(entity.name.as_deref(), Some("My Conversation"));
        assert_eq!(entity.slug.as_deref(), Some("my-convo"));
        assert!(!entity.is_private);
        assert!(!entity.is_archived);
    }

    #[test]
    fn test_entity_defaults() {
        let entity = Entity::new(EntityType::document());

        assert!(entity.is_private); // Private by default
        assert!(!entity.is_archived);
        assert!(entity.name.is_none());
        assert!(entity.slug.is_none());
        assert!(entity.user_id.is_none());
    }

    #[test]
    fn test_relation_with_metadata() {
        let metadata = serde_json::json!({
            "at_turn_id": "turn-123"
        });

        let relation = EntityRelation::new(RelationType::forked_from())
            .with_metadata(metadata.clone());

        assert_eq!(relation.relation.as_str(), "forked_from");
        assert_eq!(relation.metadata, Some(metadata));
    }
}
