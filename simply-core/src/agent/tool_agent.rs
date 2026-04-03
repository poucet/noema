//! Agent with tool support via ToolService trait

use super::context::ConversationContext;
use super::tool_service::ToolService;
use super::{Agent, ExecutionContext};
use crate::storage::document_resolver::{DocumentFormatter, DocumentResolver};
use crate::storage::ids::DocumentId;
use crate::traffic_log;
use anyhow::Result;
use async_trait::async_trait;
use llm::{ChatMessage, ChatModel, ChatPayload, ChatRequest, ContentBlock, ToolResultContent};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Events emitted during streaming agent execution.
#[derive(Debug, Clone)]
pub enum AgentStreamEvent {
    /// Text delta from LLM.
    TextDelta(String),
    /// Non-text content block (image, audio, etc.)
    ContentBlock(ContentBlock),
    /// Tool call initiated.
    ToolCallStart {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Tool call completed with result.
    ToolCallResult {
        id: String,
        content: Vec<ToolResultContent>,
    },
}

/// Optional sink for streaming events during agent execution.
pub type AgentStreamSink = mpsc::UnboundedSender<AgentStreamEvent>;

pub struct ToolAgent {
    tools: Arc<dyn ToolService>,
    max_iterations: usize,
    document_resolver: Arc<dyn DocumentResolver>,
    document_formatter: DocumentFormatter,
    execution_context: ExecutionContext,
    stream_sink: Option<AgentStreamSink>,
}

impl ToolAgent {
    pub fn new(
        tools: Arc<dyn ToolService>,
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
            stream_sink: None,
        }
    }

    /// Set a stream sink to receive events during execution.
    pub fn with_stream_sink(mut self, sink: AgentStreamSink) -> Self {
        self.stream_sink = Some(sink);
        self
    }

    pub fn execution_context(&self) -> &ExecutionContext {
        &self.execution_context
    }

    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }

    fn emit(&self, event: AgentStreamEvent) {
        if let Some(ref sink) = self.stream_sink {
            let _ = sink.send(event);
        }
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

    /// Process a single tool call via the tool service.
    async fn process_single_tool_call(
        &self,
        tool_call: &llm::ToolCall,
    ) -> Vec<ToolResultContent> {
        let args = tool_call.arguments.clone();

        // Emit tool call start
        self.emit(AgentStreamEvent::ToolCallStart {
            id: tool_call.id.clone(),
            name: tool_call.name.clone(),
            arguments: args.clone(),
        });

        let result = self
            .tools
            .call_tool(&tool_call.name, args)
            .await
            .unwrap_or_else(|e| vec![ToolResultContent::text(format!("Error: {}", e))]);

        // Emit tool call result
        self.emit(AgentStreamEvent::ToolCallResult {
            id: tool_call.id.clone(),
            content: result.clone(),
        });

        result
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
impl Agent for ToolAgent {
    async fn execute_stream(
        &self,
        context: &mut dyn ConversationContext,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> Result<()> {
        use futures::StreamExt;

        for iteration in 0..self.max_iterations {
            let tool_definitions = self.tools.get_definitions().await;
            let tool_count = tool_definitions.len();

            let messages = context.messages().await?;
            let mut request = if tool_definitions.is_empty() {
                ChatRequest::new(messages.iter())
            } else {
                ChatRequest::with_tools(messages.iter(), tool_definitions)
            };

            self.resolve_documents(&mut request).await;

            // Log what we're sending to the LLM
            tracing::info!(
                model = model.name(),
                iteration,
                message_count = messages.len(),
                tool_count,
                "sending to LLM"
            );
            for (i, msg) in messages.iter().enumerate() {
                let preview: String = msg.payload.content.iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.chars().take(40).collect::<String>()),
                        ContentBlock::ToolCall(tc) => Some(format!("[tool:{}]", tc.name)),
                        ContentBlock::ToolResult(_) => Some("[tool_result]".to_string()),
                        _ => None,
                    })
                    .next()
                    .unwrap_or_default();
                tracing::debug!(i, role = ?msg.role, preview, "  msg");
            }

            let mut stream = model.stream_chat(&request).await?;

            let mut accumulated_text = String::new();
            let mut other_blocks: Vec<ContentBlock> = Vec::new();
            let mut role = llm::api::Role::default();

            while let Some(chunk) = stream.next().await {
                role = chunk.role;
                for block in chunk.payload.content {
                    match block {
                        ContentBlock::Text { ref text } => {
                            self.emit(AgentStreamEvent::TextDelta(text.clone()));
                            accumulated_text.push_str(text);
                        }
                        other => {
                            self.emit(AgentStreamEvent::ContentBlock(other.clone()));
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
                    "ToolAgent reached max iterations ({}), stopping",
                    self.max_iterations
                );
            }
        }

        Ok(())
    }
}
