//! Conversation structure types
//!
//! This module defines types for both the legacy conversation model
//! and the new Turn/Span/Message hierarchy (Unified Content Model).
//!
//! ## Legacy Types (used during migration)
//! - `SpanType` - User or assistant span type
//! - `ConversationInfo` - Conversation metadata for listing
//! - `ThreadInfo` - Thread metadata
//! - `SpanInfo` (legacy) - Span metadata with selection state
//! - `SpanSetInfo` - SpanSet metadata
//! - `SpanSetWithContent` - SpanSet with messages
//!
//! ## New Types (Turn/Span/Message hierarchy)
//! - `TurnInfo` - A position in the conversation sequence
//! - `NewSpanInfo` - A span of messages at a turn
//! - `MessageInfo` - Individual content within a span
//! - `ViewInfo` - A path through spans

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::storage::content::StoredMessage;
use crate::storage::ids::{ContentBlockId, ConversationId, MessageContentId, MessageId, SpanId, TurnId, ViewId};

// ============================================================================
// Legacy Types (for migration compatibility)
// ============================================================================

/// Span type (user input or assistant response)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacySpanType {
    User,
    Assistant,
}

impl ToString for LegacySpanType {
    fn to_string(&self) -> String {
        match self {
            LegacySpanType::User => "user".to_string(),
            LegacySpanType::Assistant => "assistant".to_string(),
        }
    }
}

impl FromStr for LegacySpanType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "user" => Ok(LegacySpanType::User),
            "assistant" => Ok(LegacySpanType::Assistant),
            _ => Err(format!("{s} is not a valid LegacySpanType")),
        }
    }
}

/// Information about a conversation for listing/display
#[derive(Debug, Clone)]
pub struct LegacyConversationInfo {
    pub id: String,
    pub name: Option<String>,
    pub message_count: usize,
    /// Whether this conversation contains private/sensitive content
    /// When true, warns before using cloud models
    pub is_private: bool,
    /// Unix timestamp when created
    pub created_at: i64,
    /// Unix timestamp when last updated
    pub updated_at: i64,
}

/// Information about a thread (for listing threads/branches)
#[derive(Debug, Clone)]
pub struct LegacyThreadInfo {
    pub id: String,
    pub conversation_id: String,
    pub parent_span_id: Option<String>,
    pub name: Option<String>,
    pub status: String,
    pub created_at: i64,
}

/// Information about a span (one model's response within a SpanSet)
#[derive(Debug, Clone)]
pub struct LegacySpanInfo {
    pub id: String,
    pub model_id: Option<String>,
    pub message_count: usize,
    pub is_selected: bool,
    pub created_at: i64,
}

/// Information about a SpanSet (position in conversation)
#[derive(Debug, Clone)]
pub struct LegacySpanSetInfo {
    pub id: String,
    pub thread_id: String,
    pub sequence_number: i64,
    pub span_type: LegacySpanType,
    pub selected_span_id: Option<String>,
    pub created_at: i64,
}

/// A SpanSet with its selected span's messages
#[derive(Debug, Clone)]
pub struct LegacySpanSetWithContent {
    pub id: String,
    pub span_type: LegacySpanType,
    pub messages: Vec<StoredMessage>,
    pub alternates: Vec<LegacySpanInfo>,
}

// ============================================================================
// New Types: Span Role
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
            _ => MessageRole::Tool,
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

// Note: ContentType enum removed - StoredContent enum discriminant is used directly
// for content_type column values ("text", "asset_ref", "document_ref", "tool_call", "tool_result")

/// A single content item within a message
///
/// Maps directly to a row in the `message_content` table.
/// Uses `StoredContent` directly - no separate `MessageContentData` needed.
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
