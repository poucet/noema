//! Conversation structure types
//!
//! This module defines types for the Turn/Span/Message hierarchy:
//!
//! - `Turn` - Core turn data (role only), stored as `Stored<TurnId, Turn>`
//! - `Span` - Core span data, stored as `Stored<SpanId, Span>`
//! - `Message` - Individual message within a span, stored as `Stored<MessageId, Message>`
//! - `View` - A path through turns/spans (defines order), stored as `Stored<ViewId, View>`

use llm::Role;
use serde::{Deserialize, Serialize};

use crate::storage::content::StoredContent;
use crate::storage::ids::{MessageId, SpanId, TurnId, ViewId};
use crate::storage::types::Stored;


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
    role: Role,
}

impl Turn {
    /// Create a new turn with the given role
    pub fn new(role: Role) -> Self {
        Self { role }
    }

    /// Accessor for role
    pub fn role(&self) -> Role {
        self.role
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
    pub role: Role,
}

impl Message {
    /// Create a new message
    pub fn new(span_id: SpanId, sequence_number: i32, role: Role) -> Self {
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
// Conversation (REMOVED - use Entity with EntityType::conversation() instead)
// ============================================================================
//
// Conversations are now fully represented as entities:
// - Entity.entity_type = EntityType::conversation()
// - Entity.metadata["main_view_id"] = the main view ID
// - Entity.name, Entity.is_private, etc. for common fields
//
// See EntityStore for CRUD operations.

#[cfg(test)]
mod tests {
}
