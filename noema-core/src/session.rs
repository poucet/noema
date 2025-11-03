//! Session utilities for managing conversations with agents
//!
//! Provides convenient wrappers that handle context creation, transactions,
//! and commits automatically.

use crate::{Agent, ConversationContext, Transaction};
use futures::stream::Stream;
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
}

/// Context that combines committed messages with a transaction
pub struct TransactionContext<'a> {
    committed: &'a [ChatMessage],
    transaction: &'a Transaction,
}

impl<'a> TransactionContext<'a> {
    pub fn new(committed: &'a [ChatMessage], transaction: &'a Transaction) -> Self {
        Self {
            committed,
            transaction,
        }
    }
}

impl<'a> ConversationContext for TransactionContext<'a> {
    fn iter(&self) -> impl Iterator<Item = &ChatMessage> {
        self.committed.iter().chain(self.transaction.pending().iter())
    }

    fn len(&self) -> usize {
        self.committed.len() + self.transaction.len()
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
        Transaction::new()
    }

    /// Create a context that includes both committed and pending messages
    pub fn transaction_context<'a>(&'a self, transaction: &'a Transaction) -> TransactionContext<'a> {
        TransactionContext::new(&self.history, transaction)
    }

    /// Execute agent within a transaction (doesn't commit)
    pub async fn execute_in_transaction(
        &self,
        transaction: &mut Transaction,
        agent: &impl Agent,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let context = self.transaction_context(transaction);
        let messages = agent.execute(&context, model).await?;
        transaction.extend(messages.clone());
        Ok(messages)
    }

    /// Commit a transaction to the session
    pub fn commit(&mut self, transaction: Transaction) {
        let messages = transaction.commit();
        self.history.extend(messages);
    }

    /// Send a user message and execute agent (auto-commit)
    ///
    /// This is a convenience method that:
    /// 1. Adds user message to transaction
    /// 2. Creates context with pending message
    /// 3. Executes agent
    /// 4. Commits all messages
    ///
    /// # Example
    ///
    /// ```ignore
    /// let messages = session.send(&SimpleAgent, model, "Hello".into()).await?;
    /// ```
    pub async fn send(
        &mut self,
        agent: &impl Agent,
        model: Arc<dyn ChatModel + Send + Sync>,
        input: impl Into<ChatPayload>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let mut tx = self.begin();

        // Add user message
        tx.add(ChatMessage::user(input.into()));

        // Execute agent
        self.execute_in_transaction(&mut tx, agent, model).await?;

        // Get all messages before committing
        let all_messages = tx.pending().to_vec();

        // Commit
        self.commit(tx);

        Ok(all_messages)
    }

    /// Send with streaming
    ///
    /// Returns (stream, transaction) - caller must commit the transaction after consuming the stream.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (mut stream, mut tx) = session.send_stream(&agent, model, "Hello".into()).await?;
    ///
    /// while let Some(msg) = stream.next().await {
    ///     println!("{}", msg.get_text());
    ///     tx.add(msg);
    /// }
    ///
    /// session.commit(tx);
    /// ```
    pub async fn send_stream(
        &mut self,
        agent: &impl Agent,
        model: Arc<dyn ChatModel + Send + Sync>,
        input: impl Into<ChatPayload>,
    ) -> anyhow::Result<(impl Stream<Item = ChatMessage>, Transaction)> {
        let mut tx = self.begin();

        // Add user message
        tx.add(ChatMessage::user(input.into()));

        // Create context
        let context = self.transaction_context(&tx);

        // Execute stream
        let stream = agent.execute_stream(&context, model).await?;

        Ok((stream, tx))
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
    use futures::stream::{self, StreamExt};
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
            _context: &(impl ConversationContext + Sync),
            model: Arc<dyn ChatModel + Send + Sync>,
        ) -> anyhow::Result<Vec<ChatMessage>> {
            let request = ChatRequest::new(&[]);
            let response = model.chat(&request).await?;
            Ok(vec![response])
        }

        async fn execute_stream(
            &self,
            _context: &(impl ConversationContext + Sync),
            model: Arc<dyn ChatModel + Send + Sync>,
        ) -> anyhow::Result<Pin<Box<dyn Stream<Item = ChatMessage> + Send>>> {
            let request = ChatRequest::new(&[]);
            let stream = model.stream_chat(&request).await?;
            let msg_stream = stream.map(|chunk| ChatMessage::from(chunk));
            Ok(Box::pin(msg_stream))
        }
    }

    #[tokio::test]
    async fn test_session_send() {
        let mut session = Session::new();
        let model = Arc::new(MockModel);

        let messages = session.send(&TestAgent, model, "Hello").await.unwrap();

        // Should have user message + agent response
        assert_eq!(messages.len(), 2);
        assert_eq!(session.len(), 2);
    }

    #[tokio::test]
    async fn test_session_transaction() {
        let mut session = Session::new();
        let model = Arc::new(MockModel);

        let mut tx = session.begin();
        tx.add(ChatMessage::user("Hello".into()));

        let messages = session.execute_in_transaction(&mut tx, &TestAgent, model).await.unwrap();

        assert_eq!(messages.len(), 1); // Just the agent response
        assert_eq!(session.len(), 0); // Not committed yet

        session.commit(tx);
        assert_eq!(session.len(), 2); // Now committed
    }

    #[test]
    fn test_session_with_system_message() {
        let session = Session::with_system_message("You are helpful");

        assert_eq!(session.len(), 1);
        assert_eq!(session.messages()[0].role, llm::api::Role::System);
    }
}
