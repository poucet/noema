//! Agent framework — traits, context, tool service, and MCP agent implementation.

mod execution_context;
pub mod context;
pub mod in_memory_context;
pub mod tool_agent;
pub mod tool_service;

pub use context::{ConversationContext, MessagesGuard};
pub use execution_context::ExecutionContext;
pub use in_memory_context::InMemoryContext;
pub use tool_agent::{AgentStreamEvent, AgentStreamSink, ToolAgent};
pub use tool_service::{EmptyToolService, ToolService};

use anyhow::Result;
use async_trait::async_trait;
use llm::ChatModel;
use std::sync::Arc;

/// An agent that processes conversation context and adds messages.
#[async_trait]
pub trait Agent: Send + Sync {
    /// Execute agent with streaming output, adding messages to context.
    async fn execute_stream(
        &self,
        context: &mut dyn ConversationContext,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> Result<()>;
}
