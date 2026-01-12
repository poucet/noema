//! TurnStore trait for Turn/Span/Message storage operations
//!
//! This trait defines the operations for the Turn/Span/Message/View
//! conversation structure (Phase 3 Unified Content Model).

use anyhow::Result;
use async_trait::async_trait;

use super::types::{
    MessageInfo, MessageRole, MessageWithContent, SpanInfo, SpanRole, TurnInfo, TurnWithContent, ViewInfo,
};
use crate::storage::content::StoredContent;
use crate::storage::ids::{ConversationId, MessageId, SpanId, TurnId, ViewId};

/// Trait for Turn/Span/Message storage operations
#[async_trait]
pub trait TurnStore: Send + Sync {
    // ========== Turn Management ==========

    /// Add a new turn to a conversation
    ///
    /// Creates a turn at the next sequence number. Also creates a default span
    /// for the turn and selects it in the main view.
    async fn add_turn(
        &self,
        conversation_id: &ConversationId,
        role: SpanRole,
    ) -> Result<TurnInfo>;

    /// Get all turns for a conversation in sequence order
    async fn get_turns(&self, conversation_id: &ConversationId) -> Result<Vec<TurnInfo>>;

    /// Get a specific turn by ID
    async fn get_turn(&self, turn_id: &TurnId) -> Result<Option<TurnInfo>>;

    // ========== Span Management ==========

    /// Add a new span to a turn
    ///
    /// Creates an additional span at the given turn (for parallel responses
    /// or regenerations).
    async fn add_span(&self, turn_id: &TurnId, model_id: Option<&str>) -> Result<SpanInfo>;

    /// Get all spans for a turn
    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<SpanInfo>>;

    /// Get a specific span by ID
    async fn get_span(&self, span_id: &SpanId) -> Result<Option<SpanInfo>>;

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
    ) -> Result<MessageInfo>;

    /// Get all messages for a span in sequence order (metadata only)
    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageInfo>>;

    /// Get all messages for a span with content loaded
    async fn get_messages_with_content(&self, span_id: &SpanId) -> Result<Vec<MessageWithContent>>;

    /// Get a specific message by ID
    async fn get_message(&self, message_id: &MessageId) -> Result<Option<MessageInfo>>;

    // ========== View Management ==========

    /// Create a new view for a conversation
    ///
    /// If `is_main` is true, this becomes the main view. A conversation can
    /// only have one main view.
    async fn create_view(
        &self,
        conversation_id: &ConversationId,
        name: Option<&str>,
        is_main: bool,
    ) -> Result<ViewInfo>;

    /// Get all views for a conversation
    async fn get_views(&self, conversation_id: &ConversationId) -> Result<Vec<ViewInfo>>;

    /// Get the main view for a conversation
    async fn get_main_view(&self, conversation_id: &ConversationId) -> Result<Option<ViewInfo>>;

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
        name: Option<&str>,
    ) -> Result<ViewInfo>;

    /// Fork a view with custom span selections
    ///
    /// Creates a new view from the given view, copying selections up to the fork
    /// point, then applying the provided custom selections for turns at/after
    /// the fork point. This enables "splicing" - reusing spans from the original
    /// path after an edit.
    async fn fork_view_with_selections(
        &self,
        view_id: &ViewId,
        at_turn_id: &TurnId,
        name: Option<&str>,
        selections: &[(TurnId, SpanId)],
    ) -> Result<ViewInfo>;

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
        fork_name: Option<&str>,
    ) -> Result<(SpanInfo, Option<ViewInfo>)>;

    // ========== Convenience Methods ==========

    /// Add a complete user turn (turn + span + message)
    ///
    /// Creates a user turn with a single span containing the given message.
    async fn add_user_turn(
        &self,
        conversation_id: &ConversationId,
        text: &str,
    ) -> Result<(TurnInfo, SpanInfo, MessageInfo)>;

    /// Add a complete assistant turn (turn + span + message)
    ///
    /// Creates an assistant turn with a single span containing the given message.
    async fn add_assistant_turn(
        &self,
        conversation_id: &ConversationId,
        model_id: &str,
        text: &str,
    ) -> Result<(TurnInfo, SpanInfo, MessageInfo)>;
}
