//! TurnStore trait for Turn/Span/Message storage operations
//!
//! This trait defines the operations for the Turn/Span/Message/View
//! conversation structure. Turns are structural nodes; views define
//! ordering via selections.

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::content::StoredContent;
use crate::storage::ids::{MessageId, SpanId, TurnId, ViewId};
use crate::storage::types::{
    Message, MessageRole, MessageWithContent, Span, SpanRole,
    Stored, Turn, TurnWithContent, View,
};

/// Trait for Turn/Span/Message storage operations
#[async_trait]
pub trait TurnStore: Send + Sync {
    // ========== Turn Management ==========

    /// Create a new turn
    ///
    /// Creates a turn with the given role. Use select_span to add it to a view.
    async fn create_turn(&self, role: SpanRole) -> Result<Stored<TurnId, Turn>>;

    /// Get a specific turn by ID
    async fn get_turn(&self, turn_id: &TurnId) -> Result<Option<Stored<TurnId, Turn>>>;

    // ========== Span Management ==========

    /// Create a new span for a turn
    ///
    /// Creates a span at the given turn (for parallel responses or regenerations).
    async fn create_span(&self, turn_id: &TurnId, model_id: Option<&str>) -> Result<Stored<SpanId, Span>>;

    /// Get all spans for a turn
    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<Stored<SpanId, Span>>>;

    /// Get a specific span by ID
    async fn get_span(&self, span_id: &SpanId) -> Result<Option<Stored<SpanId, Span>>>;

    // ========== Message Management ==========

    /// Add a message to a span
    ///
    /// Each StoredContent item is stored in message_content:
    /// - Text → stored in content_blocks, referenced by content_block_id
    /// - AssetRef → stored directly (asset_id, mime_type, filename)
    /// - DocumentRef → stored directly (document_id, title)
    /// - ToolCall/ToolResult → stored as JSON in tool_data
    async fn add_message(
        &self,
        span_id: &SpanId,
        role: MessageRole,
        content: &[StoredContent],
    ) -> Result<Stored<MessageId, Message>>;

    /// Get all messages for a span with content loaded
    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageWithContent>>;

    /// Get a specific message by ID
    async fn get_message(&self, message_id: &MessageId) -> Result<Option<Stored<MessageId, Message>>>;

    // ========== View Management ==========

    /// Create a new view
    ///
    /// Views are linked to conversations via Conversation.main_view_id.
    async fn create_view(&self) -> Result<Stored<ViewId, View>>;

    /// Get a view by its ID
    async fn get_view(&self, view_id: &ViewId) -> Result<Option<Stored<ViewId, View>>>;

    /// Select a span for a turn within a view
    ///
    /// Updates which span is selected at the given turn for the given view.
    async fn select_span(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
        span_id: &SpanId,
    ) -> Result<()>;

    /// Get the selected span for a turn within a view
    async fn get_selected_span(&self, view_id: &ViewId, turn_id: &TurnId)
        -> Result<Option<SpanId>>;

    /// Get the full view path (all turns with their selected spans and messages)
    async fn get_view_path(&self, view_id: &ViewId) -> Result<Vec<TurnWithContent>>;

    /// Fork a view at a specific turn
    ///
    /// Creates a new view that shares selections with the original up to (but
    /// not including) the fork turn.
    async fn fork_view(
        &self,
        view_id: &ViewId,
        at_turn_id: &TurnId,
    ) -> Result<Stored<ViewId, View>>;

    /// Get the view path up to (but not including) a specific turn
    ///
    /// Returns turns with their selected spans from the start of the
    /// conversation up to but not including the specified turn.
    /// Useful for building context when editing mid-conversation.
    async fn get_view_context_at(
        &self,
        view_id: &ViewId,
        up_to_turn_id: &TurnId,
    ) -> Result<Vec<TurnWithContent>>;

    /// Edit a turn by creating a new span with new content
    ///
    /// Creates a new span at the specified turn with the given messages.
    /// If `create_fork` is true, also creates a forked view that selects
    /// this new span at the edited turn.
    ///
    /// Each message is a (role, content) pair.
    /// Returns the new span and optionally the new view.
    async fn edit_turn(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
        messages: Vec<(MessageRole, Vec<StoredContent>)>,
        model_id: Option<&str>,
        create_fork: bool,
    ) -> Result<(Stored<SpanId, Span>, Option<Stored<ViewId, View>>)>;

}
