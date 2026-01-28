//! Cross-reference types
//!
//! Types for linking any entity to any other entity with optional
//! relation type and context.

use serde::{Deserialize, Serialize};

use crate::storage::ids::EntityId;
use crate::storage::types::{EntityType, RelationType};

// ============================================================================
// EntityRef
// ============================================================================

/// A reference to any entity by type and ID
///
/// Used when you need both the type and ID together, such as for
/// resolving @mentions or building entity links.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityRef {
    /// Type of the referenced entity
    pub entity_type: EntityType,
    /// ID of the referenced entity
    pub entity_id: EntityId,
}

impl EntityRef {
    /// Create a new entity reference
    pub fn new(entity_type: EntityType, entity_id: EntityId) -> Self {
        Self {
            entity_type,
            entity_id,
        }
    }

    /// Create a reference to a conversation
    pub fn conversation(id: EntityId) -> Self {
        Self::new(EntityType::conversation(), id)
    }

    /// Create a reference to a document
    pub fn document(id: EntityId) -> Self {
        Self::new(EntityType::document(), id)
    }

    /// Create a reference to an asset
    pub fn asset(id: EntityId) -> Self {
        Self::new(EntityType::asset(), id)
    }
}

// ============================================================================
// Reference
// ============================================================================

/// Core cross-reference data
///
/// References link one entity to another with optional relation type
/// and context. Backlinks are computed by querying references where
/// `to_entity_id` matches the target.
///
/// Use with `Stored<ReferenceId, Reference>` for the full stored representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reference {
    /// Source entity that contains the reference
    pub from_entity_id: EntityId,
    /// Target entity being referenced
    pub to_entity_id: EntityId,
    /// Optional relation type (cites, mentions, derived_from, etc.)
    /// Reuses RelationType from entity module for consistency.
    pub relation_type: Option<RelationType>,
    /// Optional context text (e.g., the @mention text or surrounding snippet)
    pub context: Option<String>,
}

impl Reference {
    /// Create a new reference from one entity to another
    pub fn new(from_entity_id: EntityId, to_entity_id: EntityId) -> Self {
        Self {
            from_entity_id,
            to_entity_id,
            relation_type: None,
            context: None,
        }
    }

    /// Set the relation type
    pub fn with_relation(mut self, relation_type: RelationType) -> Self {
        self.relation_type = Some(relation_type);
        self
    }

    /// Set the context text
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Create a "cites" reference
    pub fn cites(from: EntityId, to: EntityId) -> Self {
        Self::new(from, to).with_relation(RelationType::new("cites"))
    }

    /// Create a "mentions" reference (from @mention)
    pub fn mentions(from: EntityId, to: EntityId, mention_text: impl Into<String>) -> Self {
        Self::new(from, to)
            .with_relation(RelationType::new("mentions"))
            .with_context(mention_text)
    }

    /// Create a "derived_from" reference
    pub fn derived_from(from: EntityId, to: EntityId) -> Self {
        Self::new(from, to).with_relation(RelationType::derived_from())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_ref_creation() {
        let entity_id = EntityId::new();
        let ref1 = EntityRef::conversation(entity_id.clone());

        assert_eq!(ref1.entity_type.as_str(), "conversation");
        assert_eq!(ref1.entity_id, entity_id);
    }

    #[test]
    fn test_entity_ref_types() {
        let id = EntityId::new();

        assert_eq!(EntityRef::conversation(id.clone()).entity_type.as_str(), "conversation");
        assert_eq!(EntityRef::document(id.clone()).entity_type.as_str(), "document");
        assert_eq!(EntityRef::asset(id.clone()).entity_type.as_str(), "asset");
    }

    #[test]
    fn test_reference_builder() {
        let from_id = EntityId::new();
        let to_id = EntityId::new();

        let reference = Reference::new(from_id.clone(), to_id.clone())
            .with_relation(RelationType::new("cites"))
            .with_context("See @api-design for details");

        assert_eq!(reference.from_entity_id, from_id);
        assert_eq!(reference.to_entity_id, to_id);
        assert_eq!(reference.relation_type.as_ref().map(|r| r.as_str()), Some("cites"));
        assert_eq!(reference.context.as_deref(), Some("See @api-design for details"));
    }

    #[test]
    fn test_reference_shortcuts() {
        let from_id = EntityId::new();
        let to_id = EntityId::new();

        let cites = Reference::cites(from_id.clone(), to_id.clone());
        assert_eq!(cites.relation_type.as_ref().map(|r| r.as_str()), Some("cites"));

        let mentions = Reference::mentions(from_id.clone(), to_id.clone(), "@api-design");
        assert_eq!(mentions.relation_type.as_ref().map(|r| r.as_str()), Some("mentions"));
        assert_eq!(mentions.context.as_deref(), Some("@api-design"));

        let derived = Reference::derived_from(from_id.clone(), to_id.clone());
        assert_eq!(derived.relation_type.as_ref().map(|r| r.as_str()), Some("derived_from"));
    }

    #[test]
    fn test_reference_defaults() {
        let from_id = EntityId::new();
        let to_id = EntityId::new();

        let reference = Reference::new(from_id, to_id);
        assert!(reference.relation_type.is_none());
        assert!(reference.context.is_none());
    }
}
