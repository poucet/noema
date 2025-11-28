//! Agent with dynamic MCP tool support

use crate::mcp::McpToolRegistry;
use crate::Agent;
use crate::ConversationContext;
use async_trait::async_trait;
use llm::{ChatMessage, ChatModel, ChatPayload, ChatRequest};
use std::sync::Arc;

/// Agent that dynamically uses tools from connected MCP servers.
///
/// Unlike `ToolAgent` which uses a static `ToolRegistry`, this agent
/// queries the `McpToolRegistry` on each turn to get the latest tools
/// from all connected MCP servers.
///
/// # Example
///
/// ```ignore
/// use noema_core::McpAgent;
/// use mcp::{McpRegistry, McpToolRegistry};
/// use std::sync::Arc;
/// use tokio::sync::Mutex;
///
/// let mcp_registry = Arc::new(Mutex::new(McpRegistry::load()?));
/// let tool_registry = McpToolRegistry::new(mcp_registry);
/// let agent = McpAgent::new(Arc::new(tool_registry), 10);
/// ```
pub struct McpAgent {
    tools: Arc<McpToolRegistry>,
    max_iterations: usize,
}

impl McpAgent {
    /// Create a new MCP agent
    ///
    /// # Arguments
    ///
    /// * `tools` - Dynamic MCP tool registry
    /// * `max_iterations` - Maximum number of turn cycles before stopping
    pub fn new(tools: Arc<McpToolRegistry>, max_iterations: usize) -> Self {
        Self {
            tools,
            max_iterations,
        }
    }

    /// Get reference to the tool registry
    pub fn tools(&self) -> &McpToolRegistry {
        &self.tools
    }

    /// Get maximum iterations
    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }
}

#[async_trait]
impl Agent for McpAgent {
    async fn execute(
        &self,
        context: &mut (impl ConversationContext + Send),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<()> {
        for iteration in 0..self.max_iterations {
            // Get current tool definitions dynamically
            let tool_definitions = self.tools.get_all_definitions().await;

            // Make request with tools
            let request = ChatRequest::with_tools(context.iter(), tool_definitions);

            let response = model.chat(&request).await?;
            let tool_calls = response.get_tool_calls();

            // Add response to context
            context.add(response.clone());

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                break;
            }

            // Execute all tool calls and add results to context
            for tool_call in tool_calls {
                let result = self
                    .tools
                    .call(&tool_call.name, tool_call.arguments.clone())
                    .await
                    .unwrap_or_else(|e| format!("Error: {}", e));

                let result_msg =
                    ChatMessage::user(ChatPayload::tool_result(tool_call.id.clone(), result));

                context.add(result_msg);
            }

            // Check if we've hit max iterations
            if iteration == self.max_iterations - 1 {
                tracing::warn!(
                    "McpAgent reached max iterations ({}), stopping",
                    self.max_iterations
                );
            }
        }

        Ok(())
    }

    async fn execute_stream(
        &self,
        context: &mut (impl ConversationContext + Send),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<()> {
        use futures::StreamExt;

        for iteration in 0..self.max_iterations {
            // Get current tool definitions dynamically
            let tool_definitions = self.tools.get_all_definitions().await;

            let request = ChatRequest::with_tools(context.iter(), tool_definitions);

            // Stream the response
            let mut stream = model.stream_chat(&request).await?;

            // Accumulate chunks into final message while adding to context
            let mut accumulated = ChatMessage::default();

            while let Some(chunk) = stream.next().await {
                // Add chunk as message for real-time updates
                let chunk_msg = ChatMessage::from(chunk.clone());
                context.add(chunk_msg);

                // Accumulate for tool call detection
                accumulated.payload.content.extend(chunk.payload.content);
                accumulated.role = chunk.role;
            }

            let tool_calls = accumulated.get_tool_calls();

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                break;
            }

            // Execute tools and add results to context
            for tool_call in tool_calls {
                let result = self
                    .tools
                    .call(&tool_call.name, tool_call.arguments.clone())
                    .await
                    .unwrap_or_else(|e| format!("Error: {}", e));

                let result_msg =
                    ChatMessage::user(ChatPayload::tool_result(tool_call.id.clone(), result));

                context.add(result_msg);
            }

            if iteration == self.max_iterations - 1 {
                tracing::warn!(
                    "McpAgent reached max iterations ({}), stopping",
                    self.max_iterations
                );
            }
        }

        Ok(())
    }
}
