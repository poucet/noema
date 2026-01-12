//! Multi-turn agent with tool calling support

use crate::Agent;
use crate::ConversationContext;
use anyhow::Result;
use async_trait::async_trait;
use llm::{ChatMessage, ChatModel, ChatPayload, ChatRequest, ToolRegistry};
use std::sync::Arc;

/// Multi-turn agent with tool calling support
///
/// Executes multiple turns of conversation, calling tools as needed until:
/// - The model returns a response without tool calls, OR
/// - Maximum iterations is reached
pub struct ToolAgent {
    tools: Arc<ToolRegistry>,
    max_iterations: usize,
}

impl ToolAgent {
    pub fn new(tools: Arc<ToolRegistry>, max_iterations: usize) -> Self {
        Self {
            tools,
            max_iterations,
        }
    }

    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }
}

#[async_trait]
impl Agent for ToolAgent {
    async fn execute(
        &self,
        context: &mut dyn ConversationContext,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> Result<()> {
        for iteration in 0..self.max_iterations {
            let messages = context.messages().await?;
            let request = ChatRequest::with_tools(
                messages.iter(),
                self.tools.get_all_definitions(),
            );

            let response = model.chat(&request).await?;
            let tool_calls: Vec<&llm::ToolCall> = response.get_tool_calls();

            context.add(response.clone());

            if tool_calls.is_empty() {
                break;
            }

            for tool_call in tool_calls {
                let result = self.tools
                    .call(&tool_call.name, tool_call.arguments.clone())
                    .await
                    .unwrap_or_else(|e| format!("Error: {}", e));

                let result_msg = ChatMessage::user(
                    ChatPayload::tool_result_text(tool_call.id.clone(), result)
                );

                context.add(result_msg);
            }

            if iteration == self.max_iterations - 1 {
                tracing::warn!(
                    "ToolAgent reached max iterations ({}), stopping",
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
            let messages = context.messages().await?;
            let request = ChatRequest::with_tools(
                messages.iter(),
                self.tools.get_all_definitions(),
            );

            let mut stream = model.stream_chat(&request).await?;

            let mut accumulated = ChatMessage::default();

            while let Some(chunk) = stream.next().await {
                let chunk_msg = ChatMessage::from(chunk.clone());
                context.add(chunk_msg);

                accumulated.payload.content.extend(chunk.payload.content);
                accumulated.role = chunk.role;
            }

            let tool_calls = accumulated.get_tool_calls();

            if tool_calls.is_empty() {
                break;
            }

            for tool_call in tool_calls {
                let result = self.tools
                    .call(&tool_call.name, tool_call.arguments.clone())
                    .await
                    .unwrap_or_else(|e| format!("Error: {}", e));

                let result_msg = ChatMessage::user(
                    ChatPayload::tool_result_text(tool_call.id.clone(), result)
                );

                context.add(result_msg);
            }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MessagesGuard;
    use llm::{ChatChunk, ChatPayload, ChatRequest, ToolCall, ToolDefinition};
    use futures::stream;
    use std::pin::Pin;
    use futures::stream::Stream;

    struct MockToolModel {
        call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait]
    impl ChatModel for MockToolModel {
        fn name(&self) -> &str {
            "mock-tool"
        }

        async fn chat(&self, _request: &ChatRequest) -> anyhow::Result<ChatMessage> {
            let count = self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            if count == 0 {
                Ok(ChatMessage::assistant(ChatPayload::with_tool_calls(
                    "Let me check that".to_string(),
                    vec![ToolCall {
                        id: "call_1".to_string(),
                        name: "test_tool".to_string(),
                        arguments: serde_json::json!({}),
                    }],
                )))
            } else {
                Ok(ChatMessage::assistant(ChatPayload::text("Done!")))
            }
        }

        async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>> {
            let msg = self.chat(request).await?;
            let chunk = ChatChunk {
                role: msg.role,
                payload: msg.payload,
            };
            Ok(Box::pin(stream::iter(vec![chunk])))
        }
    }

    struct MockContext {
        messages: Vec<ChatMessage>,
        pending: Vec<ChatMessage>,
    }

    #[async_trait]
    impl ConversationContext for MockContext {
        async fn messages(&mut self) -> Result<MessagesGuard<'_>> {
            Ok(MessagesGuard::new(&self.messages))
        }

        fn len(&self) -> usize {
            self.messages.len() + self.pending.len()
        }

        fn add(&mut self, message: ChatMessage) {
            self.pending.push(message);
        }

        fn pending(&self) -> &[ChatMessage] {
            &self.pending
        }

        async fn commit(&mut self) -> Result<()> {
            self.messages.append(&mut self.pending);
            Ok(())
        }
    }

    async fn mock_tool(_args: serde_json::Value) -> anyhow::Result<String> {
        Ok("tool result".to_string())
    }

    #[tokio::test]
    async fn test_tool_agent_multi_turn() {
        let mut tools = ToolRegistry::new();

        let tool_def = ToolDefinition {
            name: "test_tool".to_string(),
            description: Some("Test tool".to_string()),
            input_schema: schemars::schema_for!(()),
        };
        tools.register(tool_def, mock_tool);

        let model = MockToolModel {
            call_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        };

        let mut context = MockContext {
            messages: vec![ChatMessage::user(ChatPayload::text("Hi"))],
            pending: vec![],
        };

        let agent = ToolAgent::new(Arc::new(tools), 5);
        agent.execute(&mut context, Arc::new(model)).await.unwrap();

        // Check pending has the responses
        assert!(context.pending.len() >= 3);
    }
}
