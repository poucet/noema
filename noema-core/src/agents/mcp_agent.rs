//! Agent with dynamic MCP tool support

use crate::agents::spawn_handler::{SpawnAgentArgs, SpawnHandler, spawn_agent_tool_definition};
use crate::storage::document_resolver::{DocumentFormatter, DocumentResolver};
use crate::storage::ids::DocumentId;
use crate::mcp::McpToolRegistry;
use crate::traffic_log;
use crate::Agent;
use crate::ConversationContext;
use anyhow::Result;
use async_trait::async_trait;
use llm::{ChatMessage, ChatModel, ChatPayload, ChatRequest, ContentBlock, ToolResultContent};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Agent that dynamically uses tools from connected MCP servers.
///
/// Optionally supports a SpawnHandler for the built-in `spawn_agent` tool,
/// which allows creating subconversations for complex subtasks.
pub struct McpAgent {
    tools: Arc<McpToolRegistry>,
    max_iterations: usize,
    document_resolver: Arc<dyn DocumentResolver>,
    document_formatter: DocumentFormatter,
    spawn_handler: Option<Arc<dyn SpawnHandler>>,
    /// Current turn ID for spawn context (set during execution)
    current_turn_id: Arc<Mutex<Option<String>>>,
    /// Current span ID for spawn context (set during execution)
    current_span_id: Arc<Mutex<Option<String>>>,
}

impl McpAgent {
    pub fn new(
        tools: Arc<McpToolRegistry>,
        max_iterations: usize,
        document_resolver: Arc<dyn DocumentResolver>,
    ) -> Self {
        Self {
            tools,
            max_iterations,
            document_resolver,
            document_formatter: DocumentFormatter,
            spawn_handler: None,
            current_turn_id: Arc::new(Mutex::new(None)),
            current_span_id: Arc::new(Mutex::new(None)),
        }
    }

    /// Create an agent with spawn support enabled
    pub fn with_spawn_handler(
        tools: Arc<McpToolRegistry>,
        max_iterations: usize,
        document_resolver: Arc<dyn DocumentResolver>,
        spawn_handler: Arc<dyn SpawnHandler>,
    ) -> Self {
        Self {
            tools,
            max_iterations,
            document_resolver,
            document_formatter: DocumentFormatter,
            spawn_handler: Some(spawn_handler),
            current_turn_id: Arc::new(Mutex::new(None)),
            current_span_id: Arc::new(Mutex::new(None)),
        }
    }

    /// Set spawn context (turn/span IDs) for the current execution
    pub async fn set_spawn_context(&self, turn_id: Option<String>, span_id: Option<String>) {
        *self.current_turn_id.lock().await = turn_id;
        *self.current_span_id.lock().await = span_id;
    }

    pub fn tools(&self) -> &McpToolRegistry {
        &self.tools
    }

    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }

    /// Check if spawn_agent is enabled
    pub fn has_spawn_handler(&self) -> bool {
        self.spawn_handler.is_some()
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

    /// Get all tool definitions including built-in spawn_agent if handler is set
    async fn get_tool_definitions(&self) -> Vec<llm::ToolDefinition> {
        let mut definitions = self.tools.get_all_definitions().await;

        // Add spawn_agent if handler is available
        if self.spawn_handler.is_some() {
            definitions.push(spawn_agent_tool_definition());
        }

        definitions
    }

    /// Process a single tool call, handling spawn_agent specially
    async fn process_single_tool_call(
        &self,
        tool_call: &llm::ToolCall,
        model: &Arc<dyn ChatModel + Send + Sync>,
    ) -> Vec<ToolResultContent> {
        // Check if this is a spawn_agent call
        if tool_call.name == "spawn_agent" {
            if let Some(ref handler) = self.spawn_handler {
                // Parse spawn arguments
                match serde_json::from_value::<SpawnAgentArgs>(tool_call.arguments.clone()) {
                    Ok(args) => {
                        let turn_id = self.current_turn_id.lock().await.clone();
                        let span_id = self.current_span_id.lock().await.clone();

                        match handler
                            .spawn(
                                turn_id.as_deref().unwrap_or(""),
                                span_id.as_deref(),
                                args,
                                Arc::clone(model),
                            )
                            .await
                        {
                            Ok(result) => result.to_tool_result_content(),
                            Err(e) => {
                                vec![ToolResultContent::text(format!(
                                    "Error spawning agent: {}",
                                    e
                                ))]
                            }
                        }
                    }
                    Err(e) => {
                        vec![ToolResultContent::text(format!(
                            "Invalid spawn_agent arguments: {}",
                            e
                        ))]
                    }
                }
            } else {
                vec![ToolResultContent::text(
                    "spawn_agent is not available (no handler configured)".to_string(),
                )]
            }
        } else {
            // Regular MCP tool call
            self.tools
                .call(&tool_call.name, tool_call.arguments.clone())
                .await
                .unwrap_or_else(|e| vec![ToolResultContent::text(format!("Error: {}", e))])
        }
    }

    async fn process_tool_calls(
        &self,
        context: &mut dyn ConversationContext,
        tool_calls: Vec<&llm::ToolCall>,
        model: &Arc<dyn ChatModel + Send + Sync>,
    ) {
        for tool_call in tool_calls {
            let result_content = self.process_single_tool_call(tool_call, model).await;

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
            let tool_definitions = self.get_tool_definitions().await;

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

            self.process_tool_calls(context, tool_calls, &model).await;

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
            let tool_definitions = self.get_tool_definitions().await;

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

            self.process_tool_calls(context, tool_calls, &model).await;

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
