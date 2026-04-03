//! Conversation context abstractions
//!
//! `ConversationContext` provides async access to conversation history and allows
//! adding new messages during agent execution.

use anyhow::Result;
use async_trait::async_trait;
use llm::ChatMessage;
use std::ops::Deref;

/// Guard that provides access to resolved messages
///
/// This guard holds a reference to the internally cached messages,
/// ensuring they remain valid while the guard is held.
pub struct MessagesGuard<'a> {
    messages: &'a [ChatMessage],
}

impl<'a> MessagesGuard<'a> {
    pub fn new(messages: &'a [ChatMessage]) -> Self {
        Self { messages }
    }
}

impl<'a> Deref for MessagesGuard<'a> {
    type Target = [ChatMessage];

    fn deref(&self) -> &Self::Target {
        self.messages
    }
}

impl<'a> AsRef<[ChatMessage]> for MessagesGuard<'a> {
    fn as_ref(&self) -> &[ChatMessage] {
        self.messages
    }
}

/// Conversation message history with persistence.
///
/// Implementations buffer messages via `add()` and persist them via `commit()`.
/// For in-memory contexts, `commit()` is a no-op.
#[async_trait]
pub trait ConversationContext: Send + Sync {
    /// Get all messages (resolved for LLM consumption)
    ///
    /// This resolves any lazy content (assets, documents) and returns
    /// a guard providing access to the cached ChatMessages.
    /// Subsequent calls return the cached data without re-resolving.
    async fn messages(&mut self) -> Result<MessagesGuard<'_>>;

    /// Get count of messages in the context
    fn len(&self) -> usize;

    /// Check if context is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Add a message to the context
    fn add(&mut self, message: ChatMessage);

    /// Optional system prompt for this conversation.
    fn system_prompt(&self) -> Option<&str> {
        None
    }

    /// Persist messages to storage (no-op for in-memory contexts)
    async fn commit(&mut self) -> Result<()> {
        Ok(())
    }
}
