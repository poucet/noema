use async_trait::async_trait;
use futures::stream::Stream;
use llm::{ChatMessage, ChatModel};
use std::pin::Pin;
use std::sync::Arc;
use crate::ConversationContext;

/// An agent that processes conversation context and produces messages
///
/// Agents examine the conversation context (which includes any new user input,
/// tool results, etc.) and produce new messages in response.
///
/// # Design
///
/// The agent trait is intentionally simple - it just looks at context and
/// produces messages. The session/transaction layer handles:
/// - Adding user input to the context
/// - Managing transactions
/// - Committing to storage
///
/// This separation allows agents to be:
/// - Pure functions (no side effects)
/// - Composable (stack agents together)
/// - Flexible (triggered by user input, tool results, or anything else)
#[async_trait]
pub trait Agent {
    /// Execute agent based on current context, returning all new messages produced
    ///
    /// The agent examines the context (which should include any triggering message
    /// like user input or tool results) and produces new messages.
    ///
    /// # Arguments
    ///
    /// * `context` - Read-only view of conversation messages (committed + pending)
    /// * `model` - Arc-wrapped chat model
    ///
    /// # Returns
    ///
    /// All messages produced by the agent (typically assistant responses, but could
    /// include tool calls, tool results, etc. for multi-turn agents)
    async fn execute(
        &self,
        context: &(impl ConversationContext + Sync),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<Vec<ChatMessage>>;

    /// Execute agent with streaming output
    ///
    /// Similar to `execute`, but yields messages as they are produced.
    /// Useful for real-time UI updates during generation.
    ///
    /// # Arguments
    ///
    /// * `context` - Read-only view of conversation messages (committed + pending)
    /// * `model` - Arc-wrapped chat model
    async fn execute_stream(
        &self,
        context: &(impl ConversationContext + Sync),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = ChatMessage> + Send>>>;
}
