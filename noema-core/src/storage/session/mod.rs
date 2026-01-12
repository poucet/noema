//! Session storage traits and implementations
//!
//! This module provides:
//! - `SessionStore` trait for session management with transactions
//! - `StorageTransaction` trait for transaction semantics
//! - `MemorySession` / `MemoryTransaction` - in-memory session implementations
//! - `SqliteStore` - the main SQLite storage backend (requires `sqlite` feature)
//! - `SqliteSession` / `SqliteTransaction` - SQLite session implementations
//!
//! For conversation CRUD operations, see `ConversationManagement` in the
//! `conversation` module.

mod memory;

#[cfg(feature = "sqlite")]
mod sqlite;

use async_trait::async_trait;
use llm::ChatMessage;

use crate::ConversationContext;

pub use memory::{MemorySession, MemoryTransaction};

#[cfg(feature = "sqlite")]
pub use sqlite::{SqliteSession, SqliteStore, SqliteTransaction};

// Re-export ConversationManagement for convenience (implementation is on SqliteStore)
#[cfg(feature = "sqlite")]
pub use crate::storage::conversation::ConversationManagement;

// ============================================================================
// Traits
// ============================================================================

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

    /// Write multiple model responses as alternates in a single position
    /// Used for parallel model execution.
    /// Returns (span_set_id, Vec<span_id>) for each model's response.
    /// Default implementation falls back to just committing the first response
    /// and returns empty strings (no span support in default implementation).
    async fn commit_parallel_responses(
        &mut self,
        responses: &[(String, Vec<ChatMessage>)],
        selected_index: usize,
    ) -> anyhow::Result<(String, Vec<String>)> {
        // Default: just commit the selected response using the regular method
        if let Some((_, messages)) = responses.get(selected_index) {
            let mut tx = self.begin();
            for msg in messages {
                tx.add(msg.clone());
            }
            self.commit(tx).await?;
        }
        // Return empty span info for non-SQLite implementations
        Ok((String::new(), vec![String::new(); responses.len()]))
    }
}
