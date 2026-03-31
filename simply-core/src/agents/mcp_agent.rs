//! Agent with dynamic MCP tool support

use super::ExecutionContext;
use crate::mcp::McpToolRegistry;
use crate::storage::document_resolver::{DocumentFormatter, DocumentResolver};
use crate::storage::ids::DocumentId;
use crate::traffic_log;
use crate::Agent;
use crate::ConversationContext;
use anyhow::Result;
use async_trait::async_trait;
use llm::{ChatMessage, ChatModel, ChatPayload, ChatRequest, ContentBlock, ToolResultContent};
use std::sync::Arc;

/// Function that enriches tool call arguments before execution.
/// Takes (tool_name, arguments, execution_context) and returns enriched arguments.
pub type ToolEnricher =
    Arc<dyn Fn(&str, serde_json::Value, &ExecutionContext) -> serde_json::Value + Send + Sync>;

/// Agent that dynamically uses tools from connected MCP servers.
///
/// All tools (including spawn_agent) come from MCP servers registered
/// in the McpRegistry. The noema-mcp-core server provides spawn_agent.
///
/// An optional enricher callback can inject execution context into specific
/// tool calls (e.g., for noema-core tools that need conversation_id, turn_id, etc).
pub struct McpAgent {
    tools: Arc<McpToolRegistry>,
    max_iterations: usize,
    document_resolver: Arc<dyn DocumentResolver>,
    document_formatter: DocumentFormatter,
    execution_context: ExecutionContext,
    enricher: Option<ToolEnricher>,
}

impl McpAgent {
    pub fn new(
        tools: Arc<McpToolRegistry>,
        max_iterations: usize,
        document_resolver: Arc<dyn DocumentResolver>,
        execution_context: ExecutionContext,
    ) -> Self {
        Self {
            tools,
            max_iterations,
            document_resolver,
            document_formatter: DocumentFormatter,
            execution_context,
            enricher: None,
        }
    }

    /// Create an agent with a tool enricher callback.
    ///
    /// The enricher is called for every tool call and can modify arguments
    /// (e.g., to inject execution context for specific tools).
    pub fn with_enricher(
        tools: Arc<McpToolRegistry>,
        max_iterations: usize,
        document_resolver: Arc<dyn DocumentResolver>,
        execution_context: ExecutionContext,
        enricher: ToolEnricher,
    ) -> Self {
        Self {
            tools,
            max_iterations,
            document_resolver,
            document_formatter: DocumentFormatter,
            execution_context,
            enricher: Some(enricher),
        }
    }

    /// Get the execution context
    pub fn execution_context(&self) -> &ExecutionContext {
        &self.execution_context
    }

    pub fn tools(&self) -> &McpToolRegistry {
        &self.tools
    }

    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }

    /// Execute streaming without any tools
    pub async fn execute_stream_no_tools(
        &self,
        context: &mut dyn ConversationContext,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> Result<()> {
        use futures::StreamExt;

        let messages = context.messages().await?;
        let mut request = ChatRequest::new(messages.iter());

        self.resolve_documents(&mut request).await;

        let mut stream = model.stream_chat(&request).await?;

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

        let mut content = Vec::new();
        if !accumulated_text.is_empty() {
            content.push(ContentBlock::Text { text: accumulated_text });
        }
        content.extend(other_blocks);

        let accumulated = ChatMessage::new(role, ChatPayload::new(content));

        traffic_log::log_llm_response(model.name(), &accumulated);

        context.add(accumulated);

        Ok(())
    }

    async fn resolve_documents(&self, request: &mut ChatRequest) {
        let doc_ids: Vec<DocumentId> = request
            .get_document_refs()
            .into_iter()
            .map(DocumentId::from)
            .collect();

        if doc_ids.is_empty() {
            return;
        }

        let resolved = self.document_resolver.resolve_documents(&doc_ids).await;
        self.document_formatter.inject_documents(request, &resolved);
    }

    /// Process a single tool call via MCP registry
    async fn process_single_tool_call(
        &self,
        tool_call: &llm::ToolCall,
    ) -> Vec<ToolResultContent> {
        // Apply enricher if present
        let args = match &self.enricher {
            Some(enricher) => {
                enricher(&tool_call.name, tool_call.arguments.clone(), &self.execution_context)
            }
            None => tool_call.arguments.clone(),
        };

        self.tools
            .call(&tool_call.name, args)
            .await
            .unwrap_or_else(|e| vec![ToolResultContent::text(format!("Error: {}", e))])
    }

    async fn process_tool_calls(
        &self,
        context: &mut dyn ConversationContext,
        tool_calls: Vec<&llm::ToolCall>,
    ) {
        for tool_call in tool_calls {
            let result_content = self.process_single_tool_call(tool_call).await;

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
        context: &mut dyn ConversationContext,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> Result<()> {
        for iteration in 0..self.max_iterations {
            let tool_definitions = self.tools.get_all_definitions().await;

            let messages = context.messages().await?;
            let mut request = if tool_definitions.is_empty() {
                ChatRequest::new(messages.iter())
            } else {
                ChatRequest::with_tools(messages.iter(), tool_definitions)
            };

            self.resolve_documents(&mut request).await;

            let response = model.chat(&request).await?;
            let tool_calls = response.get_tool_calls();

            context.add(response.clone());

            if tool_calls.is_empty() {
                break;
            }

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

    async fn execute_stream(
        &self,
        context: &mut dyn ConversationContext,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> Result<()> {
        use futures::StreamExt;

        for iteration in 0..self.max_iterations {
            let tool_definitions = self.tools.get_all_definitions().await;

            let messages = context.messages().await?;
            let mut request = if tool_definitions.is_empty() {
                ChatRequest::new(messages.iter())
            } else {
                ChatRequest::with_tools(messages.iter(), tool_definitions)
            };

            self.resolve_documents(&mut request).await;

            let mut stream = model.stream_chat(&request).await?;

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

            let mut content = Vec::new();
            if !accumulated_text.is_empty() {
                content.push(ContentBlock::Text { text: accumulated_text });
            }
            content.extend(other_blocks);

            let accumulated = ChatMessage::new(role, ChatPayload::new(content));

            traffic_log::log_llm_response(model.name(), &accumulated);

            context.add(accumulated.clone());

            let tool_calls = accumulated.get_tool_calls();

            if tool_calls.is_empty() {
                break;
            }

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
