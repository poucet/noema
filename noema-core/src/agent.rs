use async_trait::async_trait;
use llm::ChatModel;
use std::sync::Arc;
use crate::ConversationContext;

/// An agent that processes conversation context and adds messages
///
/// Agents examine the conversation context (which includes any new user input,
/// tool results, etc.) and add new messages to the context directly.
///
/// # Design
///
/// The agent trait passes a mutable context, allowing agents to add messages
/// directly rather than returning them. The session/transaction layer handles:
/// - Adding user input to the context before calling agent
/// - Managing transactions
/// - Committing to storage (async)
///
/// This separation allows agents to be:
/// - Composable (stack agents together)
/// - Flexible (triggered by user input, tool results, or anything else)
/// - Simple (just mutate context, no return values)
#[async_trait]
pub trait Agent {
    /// Execute agent based on current context, adding messages to context
    ///
    /// The agent examines the context (which should include any triggering message
    /// like user input or tool results) and adds new messages directly to the context.
    ///
    /// # Arguments
    ///
    /// * `context` - Mutable context to read from and add messages to
    /// * `model` - Arc-wrapped chat model
    async fn execute(
        &self,
        context: &mut (impl ConversationContext + Send),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<()>;

    /// Execute agent with streaming output
    ///
    /// Similar to `execute`, but streams messages as they are produced.
    /// Each message chunk is immediately added to the context.
    /// Useful for real-time UI updates during generation.
    ///
    /// # Arguments
    ///
    /// * `context` - Mutable context to read from and add messages to
    /// * `model` - Arc-wrapped chat model
    async fn execute_stream(
        &self,
        context: &mut (impl ConversationContext + Send),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<()>;
}
