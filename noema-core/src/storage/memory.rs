//! In-memory session storage
//!
//! This is the default storage backend - fast but not persistent.

use super::traits::{SessionStore, StorageTransaction};
use crate::{Agent, ConversationContext};
use async_trait::async_trait;
use llm::{ChatMessage, ChatModel, ChatPayload};
use std::sync::Arc;

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

// Convenience methods for working with agents
impl MemorySession {
    /// Execute agent within a transaction (doesn't commit)
    pub async fn execute_in_transaction(
        &self,
        transaction: &mut MemoryTransaction,
        agent: &impl Agent,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<()> {
        agent.execute(transaction, model).await
    }

    /// Send a user message and execute agent (auto-commit)
    pub async fn send(
        &mut self,
        agent: &impl Agent,
        model: Arc<dyn ChatModel + Send + Sync>,
        input: impl Into<ChatPayload>,
    ) -> anyhow::Result<()> {
        let mut tx = self.begin();
        tx.add(ChatMessage::user(input.into()));
        self.execute_in_transaction(&mut tx, agent, model).await?;
        self.commit(tx).await?;
        Ok(())
    }

    /// Send with streaming - returns transaction for caller to commit
    pub async fn send_stream(
        &mut self,
        agent: &impl Agent,
        model: Arc<dyn ChatModel + Send + Sync>,
        input: impl Into<ChatPayload>,
    ) -> anyhow::Result<MemoryTransaction> {
        let mut tx = self.begin();
        tx.add(ChatMessage::user(input.into()));
        agent.execute_stream(&mut tx, model).await?;
        Ok(tx)
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
