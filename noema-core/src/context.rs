//! Conversation context abstractions
//!
//! `ConversationContext` provides access to conversation history and allows
//! adding new messages during agent execution.
//!
//! Note: This trait is intentionally synchronous for reading. Writing (adding
//! messages) is also synchronous, but committing the context may be async.

use llm::ChatMessage;

/// Access to conversation messages with mutation support
///
/// This trait provides synchronous access to conversation history and allows
/// agents to add new messages during execution. All messages must be
/// immediately available (in-memory) for reading.
///
/// # Design Note
///
/// - Reading is synchronous (keeps iteration simple)
/// - Adding messages is synchronous (buffered in memory)
/// - Committing may be async (handled at session layer)
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
    /// ```
    fn iter(&self) -> impl Iterator<Item = &ChatMessage>;

    /// Get count of messages in the context
    fn len(&self) -> usize;

    /// Check if context is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Add a message to the context
    ///
    /// This adds the message to the context's buffer. Whether it's immediately
    /// committed to storage depends on the implementation.
    fn add(&mut self, message: ChatMessage);

    /// Add multiple messages to the context
    fn extend(&mut self, messages: impl IntoIterator<Item = ChatMessage>) {
        for message in messages {
            self.add(message);
        }
    }
}
