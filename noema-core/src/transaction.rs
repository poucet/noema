//! Transaction abstraction for uncommitted messages
//!
//! Transactions provide a write buffer for messages that haven't been committed
//! to storage yet. This allows for validation, rollback, and transactional semantics.

use crate::ConversationContext;
use llm::ChatMessage;

/// A transaction buffer for uncommitted messages
///
/// Transactions combine committed history with a buffer of pending messages.
/// This allows:
/// - Reading from both committed and pending messages
/// - Adding new messages during agent execution
/// - Committing all pending messages at once
/// - Rollback on error
/// - All-or-nothing semantics for multi-step agent operations
///
/// # Example
/// ```ignore
/// let mut tx = session.begin();
/// tx.add(ChatMessage::user("Hello".into()));
/// agent.execute(&mut tx, model).await?;
///
/// // Inspect before committing
/// if is_valid(&tx) {
///     session.commit(tx).await?;
/// } else {
///     tx.rollback();
/// }
/// ```
pub struct Transaction {
    committed: Vec<ChatMessage>,
    pending: Vec<ChatMessage>,
    finalized: bool,
}

impl Transaction {
    /// Create a new transaction with committed history
    pub fn new(committed: Vec<ChatMessage>) -> Self {
        Self {
            committed,
            pending: Vec::new(),
            finalized: false,
        }
    }

    /// Get all pending messages
    pub fn pending(&self) -> &[ChatMessage] {
        &self.pending
    }

    /// Get committed messages
    pub fn committed(&self) -> &[ChatMessage] {
        &self.committed
    }

    /// Check if transaction has been finalized (committed or rolled back)
    pub fn is_finalized(&self) -> bool {
        self.finalized
    }

    /// Commit the transaction, returning all pending messages
    ///
    /// This consumes the transaction and marks it as committed.
    pub fn commit(mut self) -> Vec<ChatMessage> {
        self.finalized = true;
        std::mem::take(&mut self.pending)
    }

    /// Rollback (discard) the transaction
    ///
    /// This consumes the transaction and discards all pending messages.
    pub fn rollback(mut self) {
        self.finalized = true;
        self.pending.clear();
    }
}

impl ConversationContext for Transaction {
    fn iter(&self) -> impl Iterator<Item = &ChatMessage> {
        self.committed.iter().chain(self.pending.iter())
    }

    fn len(&self) -> usize {
        self.committed.len() + self.pending.len()
    }

    fn add(&mut self, message: ChatMessage) {
        assert!(!self.finalized, "Cannot add to finalized transaction");
        self.pending.push(message);
    }

    fn extend(&mut self, messages: impl IntoIterator<Item = ChatMessage>) {
        assert!(!self.finalized, "Cannot add to finalized transaction");
        self.pending.extend(messages);
    }
}

impl Default for Transaction {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.finalized && !self.pending.is_empty() {
            eprintln!(
                "Warning: Transaction dropped without commit/rollback ({} messages lost)",
                self.pending.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llm::{ChatPayload, ChatMessage};

    #[test]
    fn test_transaction_new() {
        let tx = Transaction::new(Vec::new());
        assert_eq!(tx.len(), 0);
        assert!(tx.is_empty());
        assert!(!tx.is_finalized());
    }

    #[test]
    fn test_transaction_add() {
        let mut tx = Transaction::new(Vec::new());
        tx.add(ChatMessage::user(ChatPayload::text("Hello")));
        assert_eq!(tx.len(), 1);
        assert!(!tx.is_empty());
    }

    #[test]
    fn test_transaction_commit() {
        let mut tx = Transaction::new(Vec::new());
        tx.add(ChatMessage::user(ChatPayload::text("Hello")));

        let messages = tx.commit();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_transaction_rollback() {
        let mut tx = Transaction::new(Vec::new());
        tx.add(ChatMessage::user(ChatPayload::text("Hello")));

        tx.rollback();
        // Transaction is consumed, can't check anything else
    }

    #[test]
    #[should_panic(expected = "Cannot add to finalized transaction")]
    fn test_transaction_add_after_manual_finalize() {
        let mut tx = Transaction::new(Vec::new());
        tx.add(ChatMessage::user(ChatPayload::text("Hello")));

        // Manually mark as finalized without consuming
        tx.finalized = true;

        // This should panic
        tx.add(ChatMessage::user(ChatPayload::text("World")));
    }
}
