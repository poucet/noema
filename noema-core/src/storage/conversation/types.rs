//! Conversation structure types for the Unified Content Model
//!
//! This module defines the Turn/Span/Message hierarchy:
//! - Turn: A position in the conversation sequence
//! - Span: An alternative response at a turn (owned by user or assistant)
//! - Message: Individual content within a span

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::storage::ids::{ContentBlockId, ConversationId, MessageId, SpanId, TurnId, ViewId};

// ============================================================================
// Span Role
// ============================================================================

/// Role identifying who owns a span (user or assistant)
///
/// Each turn can have multiple spans, but all spans at a turn share the same role.
/// The role indicates who is "speaking" at that position in the conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanRole {
    /// User input span
    User,
    /// Assistant response span
    Assistant,
}

impl SpanRole {
    /// Convert to database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            SpanRole::User => "user",
            SpanRole::Assistant => "assistant",
        }
    }

    /// Parse from database string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "user" => Some(SpanRole::User),
            "assistant" => Some(SpanRole::Assistant),
            _ => None,
        }
    }
}

// ============================================================================
// Message Role
// ============================================================================

/// Role for individual messages within a span
///
/// While spans have a SpanRole (user/assistant), individual messages can have
/// more specific roles for multi-step flows (e.g., tool calls within an assistant span).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// User message
    User,
    /// Assistant message
    Assistant,
    /// System message
    System,
    /// Tool call or result
    Tool,
}

impl MessageRole {
    /// Convert to database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool => "tool",
        }
    }

    /// Parse from database string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "user" => Some(MessageRole::User),
            "assistant" => Some(MessageRole::Assistant),
            "system" => Some(MessageRole::System),
            "tool" => Some(MessageRole::Tool),
            _ => None,
        }
    }
}

// ============================================================================
// Turn
// ============================================================================

/// A turn in a conversation - a position in the sequence
///
/// Each turn represents a point where someone "speaks" (user or assistant).
/// Turns are ordered by sequence_number within a conversation.
/// Each turn can have multiple alternative spans.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TurnInfo {
    /// Unique identifier
    pub id: TurnId,
    /// Parent conversation
    pub conversation_id: ConversationId,
    /// Role for all spans at this turn (user or assistant)
    pub role: SpanRole,
    /// Position in conversation (0-indexed, unique per conversation)
    pub sequence_number: i32,
    /// Unix timestamp when created
    pub created_at: i64,
}

// ============================================================================
// Span
// ============================================================================

/// A span within a turn - one alternative response
///
/// A span contains an ordered sequence of messages. Different spans at the
/// same turn represent alternative responses (e.g., different model outputs,
/// regenerations, or edits).
///
/// Example: Assistant turn with parallel model responses:
/// - Span A (claude): [thinking] → [tool_call] → [tool_result] → [response]
/// - Span B (gpt-4):  [response]
/// - Span C (gemini): [tool_call] → [tool_result] → [response]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpanInfo {
    /// Unique identifier
    pub id: SpanId,
    /// Parent turn
    pub turn_id: TurnId,
    /// Model that generated this span (for assistant spans)
    pub model_id: Option<String>,
    /// Number of messages in this span
    pub message_count: i32,
    /// Unix timestamp when created
    pub created_at: i64,
}

// ============================================================================
// Message
// ============================================================================

/// A message within a span
///
/// Messages are ordered within their span by sequence_number.
/// Text content is stored in ContentBlocks (referenced by content_id).
/// Tool calls and results are stored inline as JSON.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageInfo {
    /// Unique identifier
    pub id: MessageId,
    /// Parent span
    pub span_id: SpanId,
    /// Order within span (0-indexed)
    pub sequence_number: i32,
    /// Message role (can differ from span role for tool messages)
    pub role: MessageRole,
    /// Reference to text content (stored in content_blocks table)
    pub content_id: Option<ContentBlockId>,
    /// Tool calls as JSON (for assistant messages that invoke tools)
    pub tool_calls: Option<String>,
    /// Tool results as JSON (for tool response messages)
    pub tool_results: Option<String>,
    /// Unix timestamp when created
    pub created_at: i64,
}

// ============================================================================
// Input Types (for creating entities)
// ============================================================================

/// Input for creating a new message
#[derive(Clone, Debug)]
pub struct NewMessage {
    /// Message role
    pub role: MessageRole,
    /// Text content (will be stored in content_blocks)
    pub text: Option<String>,
    /// Tool calls JSON
    pub tool_calls: Option<String>,
    /// Tool results JSON
    pub tool_results: Option<String>,
}

impl NewMessage {
    /// Create a user message with text
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            text: Some(text.into()),
            tool_calls: None,
            tool_results: None,
        }
    }

    /// Create an assistant message with text
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            text: Some(text.into()),
            tool_calls: None,
            tool_results: None,
        }
    }

    /// Create an assistant message with tool calls
    pub fn assistant_with_tools(text: Option<String>, tool_calls: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            text,
            tool_calls: Some(tool_calls.into()),
            tool_results: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_results: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            text: None,
            tool_calls: None,
            tool_results: Some(tool_results.into()),
        }
    }

    /// Create a system message
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            text: Some(text.into()),
            tool_calls: None,
            tool_results: None,
        }
    }
}

// ============================================================================
// View Types
// ============================================================================

/// A view through a conversation - selects one span per turn
///
/// Views create named paths through conversations. The "main" view is the
/// default path. Additional views can be created for forks or comparisons.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewInfo {
    /// Unique identifier
    pub id: ViewId,
    /// Parent conversation
    pub conversation_id: ConversationId,
    /// Human-readable name (optional)
    pub name: Option<String>,
    /// Whether this is the main/default view
    pub is_main: bool,
    /// View this was forked from (if any)
    pub forked_from_view_id: Option<ViewId>,
    /// Turn where the fork occurred (if forked)
    pub forked_at_turn_id: Option<TurnId>,
    /// Unix timestamp when created
    pub created_at: i64,
}

/// Selection of a span within a view
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewSelection {
    /// View making the selection
    pub view_id: ViewId,
    /// Turn being selected
    pub turn_id: TurnId,
    /// Selected span at that turn
    pub span_id: SpanId,
}

// ============================================================================
// Composite Types (for queries)
// ============================================================================

/// A turn with its selected span and messages (for view path queries)
#[derive(Clone, Debug)]
pub struct TurnWithContent {
    /// The turn
    pub turn: TurnInfo,
    /// Selected span at this turn
    pub span: SpanInfo,
    /// Messages in the selected span
    pub messages: Vec<MessageInfo>,
}

/// A span with its messages
#[derive(Clone, Debug)]
pub struct SpanWithMessages {
    /// The span
    pub span: SpanInfo,
    /// Messages in the span
    pub messages: Vec<MessageInfo>,
}

// ============================================================================
// Turn Store Trait
// ============================================================================

/// Trait for Turn/Span/Message storage operations
///
/// This trait defines the operations for the new conversation structure.
/// It coexists with the legacy `ConversationStore` trait during migration.
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
    async fn add_span(
        &self,
        turn_id: &TurnId,
        model_id: Option<&str>,
    ) -> Result<SpanInfo>;

    /// Get all spans for a turn
    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<SpanInfo>>;

    /// Get a specific span by ID
    async fn get_span(&self, span_id: &SpanId) -> Result<Option<SpanInfo>>;

    // ========== Message Management ==========

    /// Add a message to a span
    ///
    /// The message text is stored in the content_blocks table and referenced
    /// by content_id.
    async fn add_message(
        &self,
        span_id: &SpanId,
        message: NewMessage,
    ) -> Result<MessageInfo>;

    /// Get all messages for a span in sequence order
    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageInfo>>;

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
    async fn get_selected_span(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
    ) -> Result<Option<SpanId>>;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_role_roundtrip() {
        for role in [SpanRole::User, SpanRole::Assistant] {
            let s = role.as_str();
            let parsed = SpanRole::from_str(s).unwrap();
            assert_eq!(parsed, role);
        }
    }

    #[test]
    fn test_message_role_roundtrip() {
        for role in [
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::System,
            MessageRole::Tool,
        ] {
            let s = role.as_str();
            let parsed = MessageRole::from_str(s).unwrap();
            assert_eq!(parsed, role);
        }
    }

    #[test]
    fn test_new_message_user() {
        let msg = NewMessage::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.text.as_deref(), Some("Hello"));
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_new_message_assistant() {
        let msg = NewMessage::assistant("Hi there");
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.text.as_deref(), Some("Hi there"));
    }

    #[test]
    fn test_new_message_tool() {
        let msg = NewMessage::tool_result(r#"{"result": "ok"}"#);
        assert_eq!(msg.role, MessageRole::Tool);
        assert!(msg.text.is_none());
        assert!(msg.tool_results.is_some());
    }
}
