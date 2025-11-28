//! In-memory session storage
//!
//! This is the default storage backend - fast but not persistent.

use super::traits::{SessionStore, StorageTransaction};
use crate::ConversationContext;
use async_trait::async_trait;
use llm::{ChatMessage, ChatPayload};

/// In-memory transaction buffer
pub struct MemoryTransaction {
    committed: Vec<ChatMessage>,
    pending: Vec<ChatMessage>,
    finalized: bool,
}

impl MemoryTransaction {
    pub fn new(committed: Vec<ChatMessage>) -> Self {
        Self {
            committed,
            pending: Vec::new(),
            finalized: false,
        }
    }
}

impl ConversationContext for MemoryTransaction {
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

impl StorageTransaction for MemoryTransaction {
    fn pending(&self) -> &[ChatMessage] {
        &self.pending
    }

    fn committed(&self) -> &[ChatMessage] {
        &self.committed
    }

    fn is_finalized(&self) -> bool {
        self.finalized
    }

    fn commit(mut self) -> Vec<ChatMessage> {
        self.finalized = true;
        std::mem::take(&mut self.pending)
    }

    fn rollback(mut self) {
        self.finalized = true;
        self.pending.clear();
    }
}

impl Drop for MemoryTransaction {
    fn drop(&mut self) {
        if !self.finalized && !self.pending.is_empty() {
            eprintln!(
                "Warning: MemoryTransaction dropped without commit/rollback ({} messages lost)",
                self.pending.len()
            );
        }
    }
}

/// In-memory session storage
///
/// Fast but not persistent - messages are lost when the session is dropped.
pub struct MemorySession {
    history: Vec<ChatMessage>,
}

impl MemorySession {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
        }
    }

    pub fn with_system_message(message: impl Into<ChatPayload>) -> Self {
        Self {
            history: vec![ChatMessage::system(message.into())],
        }
    }
}

impl Default for MemorySession {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionStore for MemorySession {
    type Transaction = MemoryTransaction;

    fn messages(&self) -> &[ChatMessage] {
        &self.history
    }

    fn messages_mut(&mut self) -> &mut Vec<ChatMessage> {
        &mut self.history
    }

    fn begin(&self) -> Self::Transaction {
        MemoryTransaction::new(self.history.clone())
    }

    async fn commit(&mut self, transaction: Self::Transaction) -> anyhow::Result<()> {
        let messages = transaction.commit();
        self.history.extend(messages);
        Ok(())
    }

    async fn clear(&mut self) -> anyhow::Result<()> {
        self.history.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_session_new() {
        let session = MemorySession::new();
        assert!(session.is_empty());
    }

    #[test]
    fn test_memory_transaction() {
        let mut tx = MemoryTransaction::new(vec![]);
        tx.add(ChatMessage::user("Hello".into()));
        assert_eq!(tx.len(), 1);
        assert_eq!(tx.pending().len(), 1);

        let msgs = tx.commit();
        assert_eq!(msgs.len(), 1);
    }

    #[tokio::test]
    async fn test_memory_session_commit() {
        let mut session = MemorySession::new();
        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));

        session.commit(tx).await.unwrap();
        assert_eq!(session.len(), 1);
    }
}
