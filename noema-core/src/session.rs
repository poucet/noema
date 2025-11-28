//! Session utilities for managing conversations with agents
//!
//! Provides convenient wrappers that handle context creation, transactions,
//! and commits automatically.

use crate::{Agent, ConversationContext, Transaction};
use llm::{ChatMessage, ChatModel, ChatPayload};
use std::sync::Arc;

/// Simple in-memory context implementation
pub struct SimpleContext {
    messages: Vec<ChatMessage>,
}

impl SimpleContext {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        Self { messages }
    }

    pub fn empty() -> Self {
        Self { messages: Vec::new() }
    }
}

impl ConversationContext for SimpleContext {
    fn iter(&self) -> impl Iterator<Item = &ChatMessage> {
        self.messages.iter()
    }

    fn len(&self) -> usize {
        self.messages.len()
    }

    fn add(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    fn extend(&mut self, messages: impl IntoIterator<Item = ChatMessage>) {
        self.messages.extend(messages);
    }
}

/// Simple session that manages conversation history and handles transactions
///
/// This is a convenience wrapper that:
/// - Stores conversation history in memory
/// - Creates contexts automatically
/// - Handles transactions and commits
///
/// # Example
///
/// ```ignore
/// let mut session = Session::new();
///
/// // Simple send - handles everything
/// let messages = session.send(&SimpleAgent, model, "Hello".into()).await?;
///
/// // Manual transaction control
/// let mut tx = session.begin();
/// tx.add(ChatMessage::user("Hello".into()));
/// let messages = session.execute_in_transaction(&mut tx, &SimpleAgent, model).await?;
/// session.commit(tx);
/// ```
pub struct Session {
    history: Vec<ChatMessage>,
}

impl Session {
    /// Create a new empty session
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
        }
    }

    /// Create a session with initial system message
    pub fn with_system_message(message: impl Into<ChatPayload>) -> Self {
        Self {
            history: vec![ChatMessage::system(message.into())],
        }
    }

    /// Get all messages in the session
    pub fn messages(&self) -> &[ChatMessage] {
        &self.history
    }

    /// Get mutable access to messages
    pub fn messages_mut(&mut self) -> &mut Vec<ChatMessage> {
        &mut self.history
    }

    /// Create a context from current history
    pub fn context(&self) -> SimpleContext {
        SimpleContext::new(self.history.clone())
    }

    /// Begin a new transaction
    pub fn begin(&self) -> Transaction {
        Transaction::new(self.history.clone())
    }

    /// Execute agent within a transaction (doesn't commit)
    pub async fn execute_in_transaction(
        &self,
        transaction: &mut Transaction,
        agent: &impl Agent,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<()> {
        agent.execute(transaction, model).await
    }

    /// Commit a transaction to the session
    ///
    /// This is async to support backend stores that require async I/O.
    /// For in-memory sessions, this is essentially a no-op but still async
    /// to maintain a consistent interface.
    pub async fn commit(&mut self, transaction: Transaction) -> anyhow::Result<()> {
        let messages = transaction.commit();
        self.history.extend(messages);
        Ok(())
    }

    /// Send a user message and execute agent (auto-commit)
    ///
    /// This is a convenience method that:
    /// 1. Creates transaction with current history
    /// 2. Adds user message to transaction
    /// 3. Executes agent (which adds messages to transaction)
    /// 4. Commits all messages
    ///
    /// # Example
    ///
    /// ```ignore
    /// session.send(&SimpleAgent, model, "Hello".into()).await?;
    /// ```
    pub async fn send(
        &mut self,
        agent: &impl Agent,
        model: Arc<dyn ChatModel + Send + Sync>,
        input: impl Into<ChatPayload>,
    ) -> anyhow::Result<()> {
        let mut tx = self.begin();

        // Add user message
        tx.add(ChatMessage::user(input.into()));

        // Execute agent (adds messages to transaction)
        self.execute_in_transaction(&mut tx, agent, model).await?;

        // Commit
        self.commit(tx).await?;

        Ok(())
    }

    /// Send with streaming
    ///
    /// Returns a transaction - caller must commit after agent completes.
    /// The agent's execute_stream adds messages to the transaction as they're produced.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut tx = session.send_stream(&agent, model, "Hello".into()).await?;
    /// // Agent has already executed and added messages to tx
    /// session.commit(tx).await?;
    /// ```
    pub async fn send_stream(
        &mut self,
        agent: &impl Agent,
        model: Arc<dyn ChatModel + Send + Sync>,
        input: impl Into<ChatPayload>,
    ) -> anyhow::Result<Transaction> {
        let mut tx = self.begin();

        // Add user message
        tx.add(ChatMessage::user(input.into()));

        // Execute agent with streaming (adds messages to transaction)
        agent.execute_stream(&mut tx, model).await?;

        Ok(tx)
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.history.clear();
    }

    /// Get message count
    pub fn len(&self) -> usize {
        self.history.len()
    }

    /// Check if session is empty
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use llm::{ChatRequest, ChatChunk};
    use futures::stream::{self, StreamExt, Stream};
    use std::pin::Pin;

    struct MockModel;

    #[async_trait]
    impl ChatModel for MockModel {
        async fn chat(&self, _request: &ChatRequest) -> anyhow::Result<ChatMessage> {
            Ok(ChatMessage::assistant(ChatPayload::text("Response")))
        }

        async fn stream_chat(&self, _request: &ChatRequest) -> anyhow::Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>> {
            let chunk = ChatChunk::assistant(ChatPayload::text("Response"));
            Ok(Box::pin(stream::iter(vec![chunk])))
        }
    }

    struct TestAgent;

    #[async_trait]
    impl Agent for TestAgent {
        async fn execute(
            &self,
            context: &mut (impl ConversationContext + Send),
            model: Arc<dyn ChatModel + Send + Sync>,
        ) -> anyhow::Result<()> {
            let request = ChatRequest::new(&[]);
            let response = model.chat(&request).await?;
            context.add(response);
            Ok(())
        }

        async fn execute_stream(
            &self,
            context: &mut (impl ConversationContext + Send),
            model: Arc<dyn ChatModel + Send + Sync>,
        ) -> anyhow::Result<()> {
            let request = ChatRequest::new(&[]);
            let mut stream = model.stream_chat(&request).await?;
            while let Some(chunk) = stream.next().await {
                context.add(ChatMessage::from(chunk));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_session_send() {
        let mut session = Session::new();
        let model = Arc::new(MockModel);

        session.send(&TestAgent, model, "Hello").await.unwrap();

        // Should have user message + agent response
        assert_eq!(session.len(), 2);
    }

    #[tokio::test]
    async fn test_session_transaction() {
        let mut session = Session::new();
        let model = Arc::new(MockModel);

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));

        session.execute_in_transaction(&mut tx, &TestAgent, model).await.unwrap();

        // Transaction has committed history + user + agent response
        assert_eq!(tx.len(), 2); // user + agent
        assert_eq!(session.len(), 0); // Not committed yet

        session.commit(tx).await.unwrap();
        assert_eq!(session.len(), 2); // Now committed
    }

    #[test]
    fn test_session_with_system_message() {
        let session = Session::with_system_message("You are helpful");

        assert_eq!(session.len(), 1);
        assert_eq!(session.messages()[0].role, llm::api::Role::System);
    }
}
