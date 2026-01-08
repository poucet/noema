//! Agent with dynamic MCP tool support

use crate::storage::document::resolver::{DocumentInjectionConfig, DocumentResolver};
use crate::mcp::McpToolRegistry;
use crate::traffic_log;
use crate::Agent;
use crate::ConversationContext;
use async_trait::async_trait;
use llm::{ChatMessage, ChatModel, ChatPayload, ChatRequest, ContentBlock, ToolResultContent};
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
    document_resolver: Arc<dyn DocumentResolver>,
    document_injection_config: DocumentInjectionConfig,
}

impl McpAgent {
    /// Create a new MCP agent
    ///
    /// # Arguments
    ///
    /// * `tools` - Dynamic MCP tool registry
    /// * `max_iterations` - Maximum number of turn cycles before stopping
    /// * `document_resolver` - Resolver for document references (required for RAG support)
    pub fn new(
        tools: Arc<McpToolRegistry>,
        max_iterations: usize,
        document_resolver: Arc<dyn DocumentResolver>,
    ) -> Self {
        Self {
            tools,
            max_iterations,
            document_resolver,
            document_injection_config: DocumentInjectionConfig::default(),
        }
    }

    /// Set the document injection configuration
    pub fn with_document_injection_config(mut self, config: DocumentInjectionConfig) -> Self {
        self.document_injection_config = config;
        self
    }

    /// Get reference to the tool registry
    pub fn tools(&self) -> &McpToolRegistry {
        &self.tools
    }

    /// Get maximum iterations
    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }

    /// Resolve document refs in a request
    async fn resolve_documents(&self, request: &mut ChatRequest) {
        self.document_resolver.resolve_request(request, &self.document_injection_config).await;
    }

    /// Process tool calls and add results to context
    async fn process_tool_calls(
        &self,
        context: &mut (impl ConversationContext + Send),
        tool_calls: Vec<&llm::ToolCall>,
    ) {
        for tool_call in tool_calls {
            let result_content = self
                .tools
                .call(&tool_call.name, tool_call.arguments.clone())
                .await
                .unwrap_or_else(|e| vec![ToolResultContent::text(format!("Error: {}", e))]);

            let result_msg =
                ChatMessage::user(ChatPayload::tool_result(tool_call.id.clone(), result_content));

            context.add(result_msg);
        }
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
            let mut request = ChatRequest::with_tools(context.iter(), tool_definitions);

            // Resolve any document refs before sending to LLM
            self.resolve_documents(&mut request).await;

            let response = model.chat(&request).await?;
            let tool_calls = response.get_tool_calls();

            // Add response to context
            context.add(response.clone());

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                break;
            }

            // Execute all tool calls and add results to context
            self.process_tool_calls(context, tool_calls).await;

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

            let mut request = ChatRequest::with_tools(context.iter(), tool_definitions);

            // Resolve any document refs before sending to LLM
            self.resolve_documents(&mut request).await;

            // Stream the response
            let mut stream = model.stream_chat(&request).await?;

            // Accumulate chunks into a single message, merging text blocks
            let mut accumulated_text = String::new();
            let mut other_blocks: Vec<ContentBlock> = Vec::new();
            let mut role = llm::api::Role::default();

            while let Some(chunk) = stream.next().await {
                role = chunk.role;
                for block in chunk.payload.content {
                    match block {
                        ContentBlock::Text { text } => {
                            accumulated_text.push_str(&text);
                        }
                        other => {
                            other_blocks.push(other);
                        }
                    }
                }
            }

            // Build the final message with merged text content
            let mut content = Vec::new();
            if !accumulated_text.is_empty() {
                content.push(ContentBlock::Text { text: accumulated_text });
            }
            content.extend(other_blocks);

            let accumulated = ChatMessage::new(role, ChatPayload::new(content));

            // Log the accumulated response
            traffic_log::log_llm_response(model.name(), &accumulated);

            // Add the complete accumulated message to context
            context.add(accumulated.clone());

            let tool_calls = accumulated.get_tool_calls();

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                break;
            }

            // Execute tools and add results to context
            self.process_tool_calls(context, tool_calls).await;

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
