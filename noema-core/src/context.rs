//! Conversation context abstractions
//!
//! `ConversationContext` provides a read-only view of conversation history.
//! Different implementations can provide filtering, windowing, etc.
//!
//! Note: This trait is intentionally synchronous and requires all messages
//! to be immediately available. For lazy-loading from storage, load messages
//! before creating the context (at the session layer).

use llm::ChatMessage;

/// Read-only view of conversation messages
///
/// This trait provides synchronous access to conversation history.
/// All messages must be immediately available (in-memory).
///
/// # Design Note
///
/// This trait is intentionally simple and synchronous:
/// - No async (keeps trait simple, easier to implement)
/// - No caching/mutation (true read-only)
/// - No lazy loading (load at session layer before creating context)
///
/// For large conversations, consider:
/// - Loading only recent messages before creating context
/// - Using windowed contexts (last N messages)
/// - Filtering contexts to reduce size
pub trait ConversationContext {
    /// Get iterator over message references
    ///
    /// Returns an iterator that yields references to messages, allowing
    /// zero-copy iteration over the conversation history.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for msg in context.iter() {
    ///     println!("{}: {}", msg.role, msg.get_text());
    /// }
    ///
    /// // Or collect to Vec if needed:
    /// let messages: Vec<ChatMessage> = context.iter().cloned().collect();
    /// ```
    fn iter(&self) -> impl Iterator<Item = &ChatMessage>;

    /// Get count of messages in the context
    fn len(&self) -> usize;

    /// Check if context is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
