//! ConversationStore trait for legacy conversation operations
//!
//! This trait defines operations for the legacy conversation structure
//! (threads, span_sets, legacy_spans, legacy_span_messages).
//! It coexists with the new `TurnStore` trait during migration.

use anyhow::Result;
use async_trait::async_trait;
use llm::api::Role;

use super::types::{
    LegacyConversationInfo, LegacySpanInfo, LegacySpanSetInfo, LegacySpanSetWithContent,
    LegacySpanType, LegacyThreadInfo,
};
use crate::storage::content::{StoredMessage, StoredPayload};

/// Trait for legacy conversation storage operations
#[async_trait]
pub trait ConversationStore: Send + Sync {
    // ========== Conversation Methods ==========

    /// List conversations for a specific user
    async fn list_conversations(&self, user_id: &str) -> Result<Vec<LegacyConversationInfo>>;

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
    async fn list_conversation_threads(&self, conversation_id: &str) -> Result<Vec<LegacyThreadInfo>>;

    /// Get a specific thread's info
    async fn get_thread(&self, thread_id: &str) -> Result<Option<LegacyThreadInfo>>;

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
    async fn create_span_set(&self, thread_id: &str, span_type: LegacySpanType) -> Result<String>;

    /// Get all SpanSets for a thread in order
    async fn get_thread_span_sets(&self, thread_id: &str) -> Result<Vec<LegacySpanSetInfo>>;

    /// Get a SpanSet with its selected span's content
    async fn get_span_set_with_content(&self, span_set_id: &str)
        -> Result<Option<LegacySpanSetWithContent>>;

    /// Get all spans for a SpanSet with message counts
    async fn get_span_set_alternates(&self, span_set_id: &str) -> Result<Vec<LegacySpanInfo>>;

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
