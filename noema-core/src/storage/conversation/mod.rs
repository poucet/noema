//! Conversation storage trait and implementations
//!
//! Provides the `ConversationStore` trait for managing conversations,
//! threads, span sets, spans, and messages.
//!
//! ## Turn/Span/Message Types
//!
//! The new types support the Turn/Span/Message structure:
//! - `TurnInfo` - A position in the conversation sequence
//! - `SpanInfo` (new) - A span of messages at a turn (one possible response)
//! - `MessageInfo` - Individual content within a span
//! - `ViewInfo` - A path through spans (selects one span per turn)
//! - `TurnStore` - Trait for the new structure

use anyhow::Result;
use async_trait::async_trait;
use llm::api::Role;
use std::str::FromStr;

use crate::storage::content::{StoredMessage, StoredPayload};

// New types for Turn/Span/Message structure
pub mod types;
pub use types::{
    MessageInfo, MessageRole, NewMessage, SpanInfo as NewSpanInfo, SpanRole, SpanWithMessages,
    TurnInfo, TurnStore, TurnWithContent, ViewInfo, ViewSelection,
};

// ============================================================================
// Types
// ============================================================================

/// Span type (user input or assistant response)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanType {
    User,
    Assistant,
}

impl ToString for SpanType {
    fn to_string(&self) -> String {
        match self {
            SpanType::User => "user".to_string(),
            SpanType::Assistant => "assistant".to_string(),
        }
    }
}

impl FromStr for SpanType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "user" => Ok(SpanType::User),
            "assistant" => Ok(SpanType::Assistant),
            _ => Err(format!("{s} is not a valid SpanType")),
        }
    }
}

/// Information about a conversation for listing/display
#[derive(Debug, Clone)]
pub struct ConversationInfo {
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
pub struct ThreadInfo {
    pub id: String,
    pub conversation_id: String,
    pub parent_span_id: Option<String>,
    pub name: Option<String>,
    pub status: String,
    pub created_at: i64,
}

/// Information about a span (one model's response within a SpanSet)
#[derive(Debug, Clone)]
pub struct SpanInfo {
    pub id: String,
    pub model_id: Option<String>,
    pub message_count: usize,
    pub is_selected: bool,
    pub created_at: i64,
}

/// Information about a SpanSet (position in conversation)
#[derive(Debug, Clone)]
pub struct SpanSetInfo {
    pub id: String,
    pub thread_id: String,
    pub sequence_number: i64,
    pub span_type: SpanType,
    pub selected_span_id: Option<String>,
    pub created_at: i64,
}

/// A SpanSet with its selected span's messages
#[derive(Debug, Clone)]
pub struct SpanSetWithContent {
    pub id: String,
    pub span_type: SpanType,
    pub messages: Vec<StoredMessage>,
    pub alternates: Vec<SpanInfo>,
}

// ============================================================================
// Trait
// ============================================================================

/// Trait for conversation storage operations
#[async_trait]
pub trait ConversationStore: Send + Sync {
    // ========== Conversation Methods ==========

    /// List conversations for a specific user
    async fn list_conversations(&self, user_id: &str) -> Result<Vec<ConversationInfo>>;

    /// Rename a conversation
    async fn rename_conversation(&self, id: &str, name: Option<&str>) -> Result<()>;

    /// Get whether a conversation is marked as private
    async fn get_conversation_private(&self, id: &str) -> Result<bool>;

    /// Set whether a conversation is marked as private
    async fn set_conversation_private(&self, id: &str, is_private: bool) -> Result<()>;

    /// Delete a conversation and all its data
    async fn delete_conversation(&self, id: &str) -> Result<()>;

    /// Load messages with StoredPayload (preserves asset refs for UI display)
    async fn load_stored_messages(&self, conversation_id: &str) -> Result<Vec<StoredMessage>>;

    // ========== Thread Methods ==========

    /// Get the main thread ID for a conversation
    async fn get_main_thread_id(&self, conversation_id: &str) -> Result<Option<String>>;

    /// Create a forked thread from a specific span
    async fn create_fork_thread(
        &self,
        conversation_id: &str,
        parent_span_id: &str,
        name: Option<&str>,
    ) -> Result<String>;

    /// Create a forked conversation from a specific span
    /// Returns (conversation_id, thread_id)
    async fn create_fork_conversation(
        &self,
        user_id: &str,
        parent_span_id: &str,
        name: Option<&str>,
    ) -> Result<(String, String)>;

    /// List all threads for a conversation
    async fn list_conversation_threads(&self, conversation_id: &str) -> Result<Vec<ThreadInfo>>;

    /// Get a specific thread's info
    async fn get_thread(&self, thread_id: &str) -> Result<Option<ThreadInfo>>;

    /// Rename a thread
    async fn rename_thread(&self, thread_id: &str, name: Option<&str>) -> Result<()>;

    /// Delete a thread (only non-main threads)
    async fn delete_thread(&self, thread_id: &str) -> Result<bool>;

    /// Get the span that a thread forks from (for walking ancestry)
    async fn get_thread_parent_span(&self, thread_id: &str) -> Result<Option<String>>;

    /// Get the span_set that contains a specific span
    async fn get_span_parent_span_set(&self, span_id: &str) -> Result<Option<String>>;

    /// Get the thread that a span_set belongs to
    async fn get_span_set_thread(&self, span_set_id: &str) -> Result<Option<String>>;

    // ========== SpanSet Methods ==========

    /// Create a new SpanSet (a position in the conversation)
    async fn create_span_set(&self, thread_id: &str, span_type: SpanType) -> Result<String>;

    /// Get all SpanSets for a thread in order
    async fn get_thread_span_sets(&self, thread_id: &str) -> Result<Vec<SpanSetInfo>>;

    /// Get a SpanSet with its selected span's content
    async fn get_span_set_with_content(&self, span_set_id: &str)
        -> Result<Option<SpanSetWithContent>>;

    /// Get all spans for a SpanSet with message counts
    async fn get_span_set_alternates(&self, span_set_id: &str) -> Result<Vec<SpanInfo>>;

    /// Set the selected span for a SpanSet
    async fn set_selected_span(&self, span_set_id: &str, span_id: &str) -> Result<()>;

    // ========== Span Methods ==========

    /// Create a new Span within a SpanSet (one model's response)
    async fn create_span(&self, span_set_id: &str, model_id: Option<&str>) -> Result<String>;

    /// Add a message to a span
    async fn add_span_message(
        &self,
        span_id: &str,
        role: Role,
        content: &StoredPayload,
    ) -> Result<String>;

    /// Get messages for a specific span
    async fn get_span_messages(&self, span_id: &str) -> Result<Vec<StoredMessage>>;

    /// Get messages for a thread with full ancestry (walks up parent_span_id chain)
    async fn get_thread_messages_with_ancestry(&self, thread_id: &str)
        -> Result<Vec<StoredMessage>>;

    // ========== Helper Methods ==========

    /// Helper: Create a user SpanSet with a single span and message
    async fn add_user_span_set(&self, thread_id: &str, content: &StoredPayload) -> Result<String>;

    /// Helper: Create an assistant SpanSet and return the span_set_id
    async fn add_assistant_span_set(&self, thread_id: &str) -> Result<String>;

    /// Helper: Add an assistant span with initial message
    async fn add_assistant_span(
        &self,
        span_set_id: &str,
        model_id: &str,
        content: &StoredPayload,
    ) -> Result<String>;
}

#[cfg(feature = "sqlite")]
pub (crate) mod sqlite;
