//! Traits for session storage backends
//!
//! These traits define the interface that both in-memory and SQLite
//! implementations must satisfy.

use async_trait::async_trait;
use llm::ChatMessage;

use crate::ConversationContext;

/// A transaction that can be committed or rolled back
///
/// This trait abstracts over the transaction mechanism, allowing
/// different backends to implement their own commit semantics.
pub trait StorageTransaction: ConversationContext + Send {
    /// Get pending (uncommitted) messages
    fn pending(&self) -> &[ChatMessage];

    /// Get committed messages (from before this transaction started)
    fn committed(&self) -> &[ChatMessage];

    /// Check if transaction has been finalized
    fn is_finalized(&self) -> bool;

    /// Consume and commit, returning pending messages
    fn commit(self) -> Vec<ChatMessage>;

    /// Consume and rollback, discarding pending messages
    fn rollback(self);
}

/// A session that manages conversation history with a storage backend
///
/// This trait abstracts over the storage mechanism, allowing both
/// in-memory and persistent (SQLite) implementations.
///
/// The core interface is intentionally minimal:
/// - `messages()` / `messages_mut()` for access
/// - `begin()` to start a transaction
/// - `commit()` to persist a transaction
/// - `clear()` to reset
///
/// Higher-level conveniences (send, send_stream, execute_in_transaction)
/// are provided by the concrete implementations, since they involve
/// generic Agent types that complicate the trait definition.
#[async_trait]
pub trait SessionStore: Send {
    /// The transaction type used by this session
    type Transaction: StorageTransaction;

    /// Get all messages in the session
    fn messages(&self) -> &[ChatMessage];

    /// Get mutable access to messages (for in-memory manipulation)
    fn messages_mut(&mut self) -> &mut Vec<ChatMessage>;

    /// Begin a new transaction
    fn begin(&self) -> Self::Transaction;

    /// Commit a transaction to storage
    ///
    /// For in-memory sessions, this just extends the history.
    /// For SQLite sessions, this writes to the database.
    async fn commit(&mut self, transaction: Self::Transaction) -> anyhow::Result<()>;

    /// Clear all history
    async fn clear(&mut self) -> anyhow::Result<()>;

    /// Get message count
    fn len(&self) -> usize {
        self.messages().len()
    }

    /// Check if session is empty
    fn is_empty(&self) -> bool {
        self.messages().is_empty()
    }
}
