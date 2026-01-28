//! Temporal query helper types
//!
//! Helper types for working with time-range queries from EntityStore.
//! The actual query is done via `EntityStore::list_entities_in_range`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::storage::traits::StoredEntity;
use crate::storage::types::EntityType;

// ============================================================================
// ActivitySummary
// ============================================================================

/// Summary of activity in a time range
///
/// Computed from a list of entities returned by `EntityStore::list_entities_in_range`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivitySummary {
    /// Start of time range (unix timestamp ms)
    pub start: i64,
    /// End of time range (unix timestamp ms)
    pub end: i64,
    /// Count of entities by type
    pub entity_counts: HashMap<EntityType, u32>,
    /// Total entities in range
    pub total_entities: u32,
}

impl ActivitySummary {
    /// Create a new empty activity summary
    pub fn new(start: i64, end: i64) -> Self {
        Self {
            start,
            end,
            entity_counts: HashMap::new(),
            total_entities: 0,
        }
    }

    /// Compute summary from a list of entities
    ///
    /// This is a pure function that aggregates entity counts by type.
    pub fn from_entities(entities: &[StoredEntity], start: i64, end: i64) -> Self {
        let mut counts: HashMap<EntityType, u32> = HashMap::new();

        for entity in entities {
            *counts.entry(entity.entity_type.clone()).or_insert(0) += 1;
        }

        Self {
            start,
            end,
            total_entities: entities.len() as u32,
            entity_counts: counts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ids::EntityId;
    use crate::storage::types::{stored_editable, Entity};

    fn make_entity(entity_type: EntityType) -> StoredEntity {
        let entity = Entity {
            entity_type,
            user_id: None,
            name: None,
            slug: None,
            is_private: true,
            is_archived: false,
            metadata: None,
        };
        stored_editable(EntityId::new(), entity, 1000, 1000)
    }

    #[test]
    fn test_activity_summary_new() {
        let summary = ActivitySummary::new(1000, 2000);
        assert_eq!(summary.start, 1000);
        assert_eq!(summary.end, 2000);
        assert!(summary.entity_counts.is_empty());
        assert_eq!(summary.total_entities, 0);
    }

    #[test]
    fn test_activity_summary_from_entities_empty() {
        let summary = ActivitySummary::from_entities(&[], 1000, 2000);
        assert_eq!(summary.total_entities, 0);
        assert!(summary.entity_counts.is_empty());
    }

    #[test]
    fn test_activity_summary_from_entities() {
        let entities = vec![
            make_entity(EntityType::conversation()),
            make_entity(EntityType::conversation()),
            make_entity(EntityType::document()),
        ];

        let summary = ActivitySummary::from_entities(&entities, 1000, 2000);

        assert_eq!(summary.total_entities, 3);
        assert_eq!(summary.entity_counts.get(&EntityType::conversation()), Some(&2));
        assert_eq!(summary.entity_counts.get(&EntityType::document()), Some(&1));
        assert_eq!(summary.entity_counts.get(&EntityType::asset()), None);
    }
}
