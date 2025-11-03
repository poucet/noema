//! Transaction abstraction for uncommitted messages
//!
//! Transactions provide a write buffer for messages that haven't been committed
//! to storage yet. This allows for validation, rollback, and transactional semantics.

use llm::ChatMessage;

/// A transaction buffer for uncommitted messages
///
/// Transactions hold messages that have been produced by agents but not yet
/// committed to persistent storage. This allows:
/// - Inspection before committing
/// - Rollback on error
/// - All-or-nothing semantics for multi-step agent operations
///
/// # Example
/// ```ignore
/// let mut tx = Transaction::new();
/// agent.execute_into(&mut tx, context, model, input).await?;
///
/// // Inspect before committing
/// if is_valid(tx.pending()) {
///     session.commit(tx);
/// } else {
///     tx.rollback();
/// }
/// ```
pub struct Transaction {
    pending: Vec<ChatMessage>,
    committed: bool,
}

impl Transaction {
    /// Create a new empty transaction
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            committed: false,
        }
    }

    /// Add a message to the transaction
    ///
    /// # Panics
    /// Panics if the transaction has already been committed or rolled back
    pub fn add(&mut self, message: ChatMessage) {
        assert!(!self.committed, "Cannot add to finalized transaction");
        self.pending.push(message);
    }

    /// Add multiple messages to the transaction
    ///
    /// # Panics
    /// Panics if the transaction has already been committed or rolled back
    pub fn extend(&mut self, messages: impl IntoIterator<Item = ChatMessage>) {
        assert!(!self.committed, "Cannot add to finalized transaction");
        self.pending.extend(messages);
    }

    /// Get all pending messages
    pub fn pending(&self) -> &[ChatMessage] {
        &self.pending
    }

    /// Number of pending messages
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Check if transaction is empty
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Check if transaction has been finalized (committed or rolled back)
    pub fn is_finalized(&self) -> bool {
        self.committed
    }

    /// Commit the transaction, returning all pending messages
    ///
    /// This consumes the transaction and marks it as committed.
    pub fn commit(mut self) -> Vec<ChatMessage> {
        self.committed = true;
        std::mem::take(&mut self.pending)
    }

    /// Rollback (discard) the transaction
    ///
    /// This consumes the transaction and discards all pending messages.
    pub fn rollback(mut self) {
        self.committed = true;
        self.pending.clear();
    }
}

impl Default for Transaction {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.committed && !self.pending.is_empty() {
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
        let tx = Transaction::new();
        assert_eq!(tx.len(), 0);
        assert!(tx.is_empty());
        assert!(!tx.is_finalized());
    }

    #[test]
    fn test_transaction_add() {
        let mut tx = Transaction::new();
        tx.add(ChatMessage::user(ChatPayload::text("Hello")));
        assert_eq!(tx.len(), 1);
        assert!(!tx.is_empty());
    }

    #[test]
    fn test_transaction_commit() {
        let mut tx = Transaction::new();
        tx.add(ChatMessage::user(ChatPayload::text("Hello")));

        let messages = tx.commit();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_transaction_rollback() {
        let mut tx = Transaction::new();
        tx.add(ChatMessage::user(ChatPayload::text("Hello")));

        tx.rollback();
        // Transaction is consumed, can't check anything else
    }

    #[test]
    #[should_panic(expected = "Cannot add to finalized transaction")]
    fn test_transaction_add_after_manual_finalize() {
        let mut tx = Transaction::new();
        tx.add(ChatMessage::user(ChatPayload::text("Hello")));

        // Manually mark as committed without consuming
        tx.committed = true;

        // This should panic
        tx.add(ChatMessage::user(ChatPayload::text("World")));
    }
}
