//! Conversation structure types
//!
//! This module defines types for the Turn/Span/Message hierarchy:
//!
//! - `TurnInfo` - A position in the conversation sequence
//! - `SpanInfo` - A span of messages at a turn (one alternative)
//! - `MessageInfo` - Individual message within a span
//! - `ViewInfo` - A path through spans (main view or fork)

use serde::{Deserialize, Serialize};

use crate::storage::ids::{ConversationId, MessageContentId, MessageId, SpanId, TurnId, ViewId};

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
    /// Get static string representation (zero allocation)
    pub const fn as_str(&self) -> &'static str {
        match self {
            SpanRole::User => "user",
            SpanRole::Assistant => "assistant",
        }
    }
}

impl std::fmt::Display for SpanRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SpanRole {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(SpanRole::User),
            "assistant" => Ok(SpanRole::Assistant),
            _ => Err(()),
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
    /// Get static string representation (zero allocation)
    pub const fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool => "tool",
        }
    }
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for MessageRole {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "system" => Ok(MessageRole::System),
            "tool" => Ok(MessageRole::Tool),
            _ => Err(()),
        }
    }
}

/// Convert from LLM Role to MessageRole
impl From<llm::api::Role> for MessageRole {
    fn from(role: llm::api::Role) -> Self {
        match role {
            llm::api::Role::User => MessageRole::User,
            llm::api::Role::Assistant => MessageRole::Assistant,
            llm::api::Role::System => MessageRole::System,
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
/// Content is stored in the `message_content` table as individual items.
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
    /// Unix timestamp when created
    pub created_at: i64,
}

/// A message with its content items loaded
#[derive(Clone, Debug)]
pub struct MessageWithContent {
    /// The message metadata
    pub message: MessageInfo,
    /// Content items in order
    pub content: Vec<MessageContentInfo>,
}

/// A single content item within a message
///
/// Maps directly to a row in the `message_content` table.
/// Uses `StoredContent` directly - refs-only design.
#[derive(Clone, Debug)]
pub struct MessageContentInfo {
    /// Unique identifier
    pub id: MessageContentId,
    /// Parent message
    pub message_id: MessageId,
    /// Order within message (0-indexed)
    pub sequence_number: i32,
    /// Content (uses StoredContent - refs-only)
    pub content: crate::storage::content::StoredContent,
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
    /// Messages in the selected span (with content loaded)
    pub messages: Vec<MessageWithContent>,
}

/// A span with its messages
#[derive(Clone, Debug)]
pub struct SpanWithMessages {
    /// The span
    pub span: SpanInfo,
    /// Messages in the span (with content loaded)
    pub messages: Vec<MessageWithContent>,
}

// ============================================================================
// Conversation Info (for listing)
// ============================================================================

/// Information about a conversation for listing/display
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationInfo {
    /// Unique identifier
    pub id: ConversationId,
    /// Human-readable name/title
    pub name: Option<String>,
    /// Number of turns in the conversation
    pub turn_count: usize,
    /// Whether this conversation contains private/sensitive content
    pub is_private: bool,
    /// Unix timestamp when created
    pub created_at: i64,
    /// Unix timestamp when last updated
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_role_roundtrip() {
        for role in [SpanRole::User, SpanRole::Assistant] {
            let s = role.as_str();
            let parsed: SpanRole = s.parse().unwrap();
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
            let parsed: MessageRole = s.parse().unwrap();
            assert_eq!(parsed, role);
        }
    }
}
