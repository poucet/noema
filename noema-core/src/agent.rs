//! Agent trait for LLM-based conversation processing

use anyhow::Result;
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
/// directly rather than returning them. The context handles:
/// - Resolving messages for LLM (async)
/// - Buffering new messages as pending
/// - Committing to storage (async)
#[async_trait]
pub trait Agent: Send + Sync {
    /// Execute agent based on current context, adding messages to context
    async fn execute(
        &self,
        context: &mut dyn ConversationContext,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> Result<()>;

    /// Execute agent with streaming output
    async fn execute_stream(
        &self,
        context: &mut dyn ConversationContext,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> Result<()>;
}
