//! TemporalStore trait for time-based entity queries
//!
//! Queries the `entities` table only. Content loading is the caller's
//! responsibility via domain-specific stores (TurnStore, DocumentStore, etc.).
//!
//! This design supports hierarchical storage where different entity types
//! may live in different backends.

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::UserId;
use crate::storage::types::temporal::{ActivitySummary, TemporalEntity, TemporalQuery};

/// Trait for time-based entity queries
///
/// Queries entities via the unified `entities` table only. Content loading
/// is explicitly not part of this trait to support hierarchical storage
/// architectures where different entity types may live in different stores.
///
/// # Use Cases
///
/// - **Search/Browse**: Find recently updated entities
/// - **Activity Feed**: Show what the user has been working on
/// - **Context Building**: Caller loads content via appropriate stores
#[async_trait]
pub trait TemporalStore: Send + Sync {
    /// Query entities by time range
    ///
    /// Returns entities updated within the specified time range, ordered by
    /// `updated_at` descending (most recent first).
    ///
    /// # Arguments
    /// - `user_id`: Scope query to this user's entities
    /// - `query`: Query parameters (time range, entity types, limit)
    ///
    /// # Content Loading
    /// This method returns entity metadata only. To load content, use the
    /// appropriate domain store (TurnStore for conversations, DocumentStore
    /// for documents, etc.) with the returned entity IDs.
    async fn query_entities(
        &self,
        user_id: &UserId,
        query: &TemporalQuery,
    ) -> Result<Vec<TemporalEntity>>;

    /// Get activity summary for a time range
    ///
    /// Returns aggregate entity counts by type.
    ///
    /// # Arguments
    /// - `user_id`: Scope summary to this user's entities
    /// - `start`: Start of time range (unix timestamp ms)
    /// - `end`: End of time range (unix timestamp ms)
    async fn get_activity_summary(
        &self,
        user_id: &UserId,
        start: i64,
        end: i64,
    ) -> Result<ActivitySummary>;
}
