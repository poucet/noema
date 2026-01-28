//! TurnStore trait for Turn/Span/Message storage operations
//!
//! This trait defines the operations for the Turn/Span/Message
//! conversation structure. Turns are structural nodes; conversations
//! define ordering via selections.

use anyhow::Result;
use async_trait::async_trait;

use llm::Role;
use crate::storage::content::StoredContent;
use crate::storage::ids::{ConversationId, MessageId, SpanId, TurnId};
use crate::storage::types::{
    Message, MessageWithContent, Span,
    Stored, Turn, TurnWithContent,
};

/// Stored representation of a Turn (immutable)
pub type StoredTurn = Stored<TurnId, Turn>;

/// Stored representation of a Span (immutable)
pub type StoredSpan = Stored<SpanId, Span>;

/// Stored representation of a Message (immutable)
pub type StoredMessage = Stored<MessageId, Message>;

/// Trait for Turn/Span/Message storage operations
#[async_trait]
pub trait TurnStore: Send + Sync {
    // ========== Turn Management ==========

    /// Create a new turn
    ///
    /// Creates a turn with the given role. Use select_span to add it to a view.
    async fn create_turn(&self, role: Role) -> Result<StoredTurn>;

    /// Get a specific turn by ID
    async fn get_turn(&self, turn_id: &TurnId) -> Result<Option<StoredTurn>>;

    // ========== Span Management ==========

    /// Create a new span for a turn
    ///
    /// Creates a span at the given turn (for parallel responses or regenerations).
    async fn create_span(&self, turn_id: &TurnId, model_id: Option<&str>) -> Result<StoredSpan>;

    /// Get all spans for a turn
    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<StoredSpan>>;

    /// Get a specific span by ID
    async fn get_span(&self, span_id: &SpanId) -> Result<Option<StoredSpan>>;

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
        role: Role,
        content: &[StoredContent],
    ) -> Result<StoredMessage>;

    /// Get all messages for a span with content loaded
    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageWithContent>>;

    /// Get a specific message by ID
    async fn get_message(&self, message_id: &MessageId) -> Result<Option<StoredMessage>>;

    // ========== Selection Management ==========
    //
    // Selections link turns to conversations. Each conversation has its own
    // linear sequence of (turn, selected_span) pairs.

    /// Select a span for a turn within a conversation
    ///
    /// Updates which span is selected at the given turn for the given conversation.
    /// If this is a new turn for the conversation, it's appended to the sequence.
    async fn select_span(
        &self,
        conversation_id: &ConversationId,
        turn_id: &TurnId,
        span_id: &SpanId,
    ) -> Result<()>;

    /// Get the selected span for a turn within a conversation
    async fn get_selected_span(
        &self,
        conversation_id: &ConversationId,
        turn_id: &TurnId,
    ) -> Result<Option<SpanId>>;

    /// Get the full conversation path (all turns with their selected spans and messages)
    async fn get_conversation_path(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Vec<TurnWithContent>>;

    /// Get the conversation path up to (but not including) a specific turn
    ///
    /// Returns turns with their selected spans from the start of the
    /// conversation up to but not including the specified turn.
    /// Useful for building context when editing mid-conversation.
    async fn get_context_at(
        &self,
        conversation_id: &ConversationId,
        up_to_turn_id: &TurnId,
    ) -> Result<Vec<TurnWithContent>>;

    /// Copy selections from one conversation to another up to a specific turn
    ///
    /// Used when forking: copies the turn sequence from source to target.
    /// If `include_turn` is true, copies up to and including the turn.
    /// If `include_turn` is false, copies up to but not including the turn.
    async fn copy_selections(
        &self,
        from_conversation_id: &ConversationId,
        to_conversation_id: &ConversationId,
        up_to_turn_id: &TurnId,
        include_turn: bool,
    ) -> Result<usize>;

    /// Get the number of turns in a conversation
    async fn get_turn_count(&self, conversation_id: &ConversationId) -> Result<usize>;
}
