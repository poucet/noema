//! Conversation structure types
//!
//! This module defines types for the Turn/Span/Message hierarchy:
//!
//! - `Turn` - Core turn data (role only), stored as `Stored<TurnId, Turn>`
//! - `Span` - Core span data, stored as `Stored<SpanId, Span>`
//! - `Message` - Individual message within a span, stored as `Stored<MessageId, Message>`
//! - `View` - A path through turns/spans (defines order), stored as `Stored<ViewId, View>`

use serde::{Deserialize, Serialize};

use crate::storage::content::StoredContent;
use crate::storage::ids::{MessageId, SpanId, TurnId, ViewId};
use crate::storage::types::Stored;

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

/// Core turn data - a structural node that can have multiple spans
///
/// Turns are independent entities; views link them together and define order.
/// A turn represents a point where someone "speaks" (user or assistant).
/// Multiple views can reference the same turn through view_selections.
///
/// Use with `Stored<TurnId, Turn>` for the full stored representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Turn {
    /// Role for all spans at this turn (user or assistant)
    pub role: SpanRole,
}

impl Turn {
    /// Create a new turn with the given role
    pub fn new(role: SpanRole) -> Self {
        Self { role }
    }

    /// Create a user turn
    pub fn user() -> Self {
        Self::new(SpanRole::User)
    }

    /// Create an assistant turn
    pub fn assistant() -> Self {
        Self::new(SpanRole::Assistant)
    }
}

// ============================================================================
// Span
// ============================================================================

/// Core span data - one alternative response within a turn
///
/// A span contains an ordered sequence of messages. Different spans at the
/// same turn represent alternative responses (e.g., different model outputs,
/// regenerations, or edits).
///
/// Example: Assistant turn with parallel model responses:
/// - Span A (claude): [thinking] → [tool_call] → [tool_result] → [response]
/// - Span B (gpt-4):  [response]
/// - Span C (gemini): [tool_call] → [tool_result] → [response]
///
/// Use with `Stored<SpanId, Span>` for the full stored representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Span {
    /// Model that generated this span (for assistant spans)
    pub model_id: Option<String>,
    /// Number of messages in this span
    pub message_count: i32,
}

impl Span {
    /// Create a new span
    pub fn new(model_id: Option<String>, message_count: i32) -> Self {
        Self {
            model_id,
            message_count,
        }
    }

    /// Create a span with a model ID
    pub fn with_model(model_id: impl Into<String>) -> Self {
        Self {
            model_id: Some(model_id.into()),
            message_count: 0,
        }
    }

    /// Create a span without a model ID (e.g., for user spans)
    pub fn without_model() -> Self {
        Self {
            model_id: None,
            message_count: 0,
        }
    }
}

// ============================================================================
// Message
// ============================================================================

/// Core message data within a span
///
/// Messages are ordered within their span by sequence_number.
/// Content is stored in the `message_content` table as individual items.
///
/// Use with `Stored<MessageId, Message>` for the full stored representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    /// Parent span
    pub span_id: SpanId,
    /// Order within span (0-indexed)
    pub sequence_number: i32,
    /// Message role (can differ from span role for tool messages)
    pub role: MessageRole,
}

impl Message {
    /// Create a new message
    pub fn new(span_id: SpanId, sequence_number: i32, role: MessageRole) -> Self {
        Self {
            span_id,
            sequence_number,
            role,
        }
    }
}

/// A message with its content items loaded
#[derive(Clone, Debug)]
pub struct MessageWithContent {
    /// The message metadata
    pub message: Stored<MessageId, Message>,
    /// Content items in order (StoredContent refs)
    pub content: Vec<StoredContent>,
}

// ============================================================================
// View Types
// ============================================================================

/// Fork origin information - where a view was forked from
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForkInfo {
    /// View this was forked from
    pub from_view_id: ViewId,
    /// Turn where the fork occurred
    pub at_turn_id: TurnId,
}

/// Core view data - selects one span per turn
///
/// Views are entities that conversations point to. The "main" view is stored
/// in Conversation.main_view_id. Forked views track their origin.
///
/// Use with `Stored<ViewId, View>` for the full stored representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct View {
    /// Fork origin (None for main views, Some for forked views)
    pub fork: Option<ForkInfo>,
    /// Number of turns selected in this view
    pub turn_count: usize,
}

impl View {
    /// Create a new main view (not forked)
    pub fn new() -> Self {
        Self {
            fork: None,
            turn_count: 0,
        }
    }

    /// Create a forked view
    pub fn forked(from_view_id: ViewId, at_turn_id: TurnId, turn_count: usize) -> Self {
        Self {
            fork: Some(ForkInfo {
                from_view_id,
                at_turn_id,
            }),
            turn_count,
        }
    }
}

impl Default for View {
    fn default() -> Self {
        Self::new()
    }
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
    pub turn: Stored<TurnId, Turn>,
    /// Selected span at this turn
    pub span: Stored<SpanId, Span>,
    /// Messages in the selected span (with content loaded)
    pub messages: Vec<MessageWithContent>,
}

// ============================================================================
// Conversation
// ============================================================================

/// Core conversation data
///
/// Use with `Stored<ConversationId, Conversation>` for the full stored representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    /// Human-readable name/title
    pub name: Option<String>,
    /// The main view for this conversation
    pub main_view_id: ViewId,
    /// Whether this conversation contains private/sensitive content
    pub is_private: bool,
}

impl Conversation {
    /// Create a new conversation with a main view
    pub fn new(main_view_id: ViewId) -> Self {
        Self {
            name: None,
            main_view_id,
            is_private: false,
        }
    }

    /// Set the conversation name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Mark as private
    pub fn private(mut self) -> Self {
        self.is_private = true;
        self
    }
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
