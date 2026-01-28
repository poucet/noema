//! TemporalStore trait for time-based entity queries
//!
//! Provides time-range queries and activity summaries for:
//! - Slash command/search (lazy loading)
//! - Manual context injection (eager loading)
//! - Future MCP/RAG integration (eager loading)

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::UserId;
use crate::storage::types::temporal::{ActivitySummary, TemporalEntity, TemporalQuery};

/// Trait for time-based entity queries
///
/// Queries entities via the unified `entities` table (which has temporal indexes),
/// then optionally resolves content from domain-specific tables based on entity type.
///
/// # Query Flow
///
/// 1. Base query on `entities` table filtered by user, time range, and entity types
/// 2. If `include_content = true`, resolve content previews by dispatching to:
///    - `conversation` → latest message via views → turns → spans → messages
///    - `document` → latest revision → content_blocks
///    - `asset` → metadata only (size, mime_type), never blobs
///
/// # Use Cases
///
/// - **Search/Browse** (`include_content: false`): Fast metadata-only queries
/// - **Injection/RAG** (`include_content: true`): Full content for LLM context
#[async_trait]
pub trait TemporalStore: Send + Sync {
    /// Query entities by time range
    ///
    /// Returns entities updated within the specified time range, ordered by
    /// `updated_at` descending (most recent first).
    ///
    /// # Arguments
    /// - `user_id`: Scope query to this user's entities
    /// - `query`: Query parameters (time range, entity types, content flag, limit)
    ///
    /// # Content Loading
    /// - If `query.include_content = false`: `content_preview` will be `None`
    /// - If `query.include_content = true`: `content_preview` populated for each entity
    async fn query_entities(
        &self,
        user_id: &UserId,
        query: &TemporalQuery,
    ) -> Result<Vec<TemporalEntity>>;

    /// Get activity summary for a time range
    ///
    /// Returns aggregate statistics without loading full content:
    /// - Entity counts by type
    /// - Total messages created
    /// - Total document revisions
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

    /// Render activity as markdown for LLM context injection
    ///
    /// Formats temporal query results as markdown suitable for including
    /// in an LLM prompt. Groups entities by type with timestamps.
    ///
    /// # Arguments
    /// - `user_id`: Scope to this user's entities
    /// - `query`: Query parameters (always loads content for rendering)
    /// - `max_chars`: Optional character limit for output (truncates oldest first)
    ///
    /// # Output Format
    /// ```markdown
    /// ## Recent Activity (Jan 15 - Jan 22)
    ///
    /// ### Conversations
    /// - **Project Planning** (updated 2h ago): "Let's discuss the API design..."
    /// - **Bug Triage** (updated 1d ago): "The issue is in the parser..."
    ///
    /// ### Documents
    /// - **API Design** (updated 3h ago): "# Overview\n\nThis document..."
    /// ```
    async fn render_activity_context(
        &self,
        user_id: &UserId,
        query: &TemporalQuery,
        max_chars: Option<u32>,
    ) -> Result<String>;
}
