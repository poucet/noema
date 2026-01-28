//! Temporal query types
//!
//! Types for time-based entity queries. TemporalStore operates only on the
//! entities table - content loading is the caller's responsibility via
//! domain-specific stores (TurnStore, DocumentStore, etc.).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::storage::ids::EntityId;
use crate::storage::types::EntityType;

// ============================================================================
// TemporalQuery
// ============================================================================

/// Query parameters for time-based entity search
///
/// The caller controls the time range - no hardcoded "recent" concept.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporalQuery {
    /// Start of time range (unix timestamp ms)
    pub start: i64,
    /// End of time range (unix timestamp ms)
    pub end: i64,
    /// Filter by entity types (None = all types)
    pub entity_types: Option<Vec<EntityType>>,
    /// Maximum number of results (None = no limit)
    pub limit: Option<u32>,
}

impl TemporalQuery {
    /// Create a new temporal query for a time range
    pub fn new(start: i64, end: i64) -> Self {
        Self {
            start,
            end,
            entity_types: None,
            limit: None,
        }
    }

    /// Filter to specific entity types
    pub fn with_types(mut self, types: Vec<EntityType>) -> Self {
        self.entity_types = Some(types);
        self
    }

    /// Limit number of results
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
}

// ============================================================================
// TemporalEntity
// ============================================================================

/// An entity returned from a temporal query
///
/// Contains entity metadata only. Content loading is the caller's
/// responsibility via domain-specific stores.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporalEntity {
    /// Entity ID
    pub entity_id: EntityId,
    /// Type of entity (conversation, document, asset, etc.)
    pub entity_type: EntityType,
    /// Display name (if set)
    pub name: Option<String>,
    /// When entity was created (unix timestamp ms)
    pub created_at: i64,
    /// When entity was last updated (unix timestamp ms)
    pub updated_at: i64,
}

// ============================================================================
// ActivitySummary
// ============================================================================

/// Summary of activity in a time range
///
/// Provides entity counts only. Detailed statistics (message counts, etc.)
/// require querying domain-specific stores.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temporal_query_builder() {
        let query = TemporalQuery::new(1000, 2000)
            .with_types(vec![EntityType::conversation(), EntityType::document()])
            .with_limit(50);

        assert_eq!(query.start, 1000);
        assert_eq!(query.end, 2000);
        assert_eq!(query.entity_types.as_ref().unwrap().len(), 2);
        assert_eq!(query.limit, Some(50));
    }

    #[test]
    fn test_activity_summary_new() {
        let summary = ActivitySummary::new(1000, 2000);
        assert_eq!(summary.start, 1000);
        assert_eq!(summary.end, 2000);
        assert!(summary.entity_counts.is_empty());
        assert_eq!(summary.total_entities, 0);
    }
}
