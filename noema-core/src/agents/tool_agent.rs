//! Multi-turn agent with tool calling support

use crate::Agent;
use crate::ConversationContext;
use async_trait::async_trait;
use futures::stream::Stream;
use llm::{ChatMessage, ChatModel, ChatPayload, ChatRequest, ToolRegistry};
use std::pin::Pin;
use std::sync::Arc;

/// Multi-turn agent with tool calling support
///
/// Executes multiple turns of conversation, calling tools as needed until:
/// - The model returns a response without tool calls, OR
/// - Maximum iterations is reached
///
/// # Example
///
/// ```ignore
/// use noema_core::{ToolAgent, Agent};
/// use llm::ToolRegistry;
///
/// let mut tools = ToolRegistry::new();
/// tools.register(search_definition, search_tool);
/// tools.register(calc_definition, calc_tool);
///
/// let agent = ToolAgent::new(Arc::new(tools), 5);
/// let messages = agent.execute(&context, &model).await?;
/// // messages contains: [assistant_with_tool_calls, tool_results, final_assistant_response]
/// ```
pub struct ToolAgent {
    tools: Arc<ToolRegistry>,
    max_iterations: usize,
}

impl ToolAgent {
    /// Create a new tool agent
    ///
    /// # Arguments
    ///
    /// * `tools` - Registry of available tools (wrapped in Arc for sharing)
    /// * `max_iterations` - Maximum number of turn cycles before stopping
    pub fn new(tools: Arc<ToolRegistry>, max_iterations: usize) -> Self {
        Self {
            tools,
            max_iterations,
        }
    }

    /// Get reference to the tool registry
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Get maximum iterations
    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }
}

#[async_trait]
impl Agent for ToolAgent {
    async fn execute(
        &self,
        context: &(impl ConversationContext + Sync),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let mut all_messages = Vec::new();
        let mut working_context: Vec<ChatMessage> = context.iter().cloned().collect();

        for iteration in 0..self.max_iterations {
            // Make request with tools
            let request = ChatRequest::with_tools(
                working_context.iter(),
                self.tools.get_all_definitions(),
            );

            let response = model.chat(&request).await?;
            let tool_calls = response.get_tool_calls();

            // Add response to working context and output
            working_context.push(response.clone());
            all_messages.push(response.clone());

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                break;
            }

            // Execute all tool calls
            for tool_call in tool_calls {
                let result = self.tools
                    .call(&tool_call.name, tool_call.arguments.clone())
                    .await
                    .unwrap_or_else(|e| format!("Error: {}", e));

                let result_msg = ChatMessage::user(
                    ChatPayload::tool_result(tool_call.id.clone(), result)
                );

                working_context.push(result_msg.clone());
                all_messages.push(result_msg);
            }

            // Check if we've hit max iterations
            if iteration == self.max_iterations - 1 {
                tracing::warn!(
                    "ToolAgent reached max iterations ({}), stopping",
                    self.max_iterations
                );
            }
        }

        Ok(all_messages)
    }

    async fn execute_stream(
        &self,
        context: &(impl ConversationContext + Sync),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = ChatMessage> + Send>>> {
        use futures::StreamExt;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let tools = self.tools.clone();
        let max_iterations = self.max_iterations;
        let mut working_context: Vec<ChatMessage> = context.iter().cloned().collect();

        // Now we can spawn because we own the model (Arc)
        tokio::spawn(async move {
            for iteration in 0..max_iterations {
                let request = ChatRequest::with_tools(
                    working_context.iter(),
                    tools.get_all_definitions(),
                );

                // Stream the response
                let stream = match model.stream_chat(&request).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!("Error streaming chat: {}", e);
                        break;
                    }
                };

                // Accumulate chunks into final message
                let mut accumulated = ChatMessage::default();
                tokio::pin!(stream);

                while let Some(chunk) = stream.next().await {
                    // Send chunk as message for real-time updates
                    let chunk_msg = ChatMessage::from(chunk.clone());
                    let _ = tx.send(chunk_msg);

                    // Accumulate for tool call detection
                    accumulated.payload.content.extend(chunk.payload.content);
                    accumulated.role = chunk.role;
                }

                let tool_calls = accumulated.get_tool_calls();
                working_context.push(accumulated.clone());

                // If no tool calls, we're done
                if tool_calls.is_empty() {
                    break;
                }

                // Execute tools and send results
                for tool_call in tool_calls {
                    let result = tools
                        .call(&tool_call.name, tool_call.arguments.clone())
                        .await
                        .unwrap_or_else(|e| format!("Error: {}", e));

                    let result_msg = ChatMessage::user(
                        ChatPayload::tool_result(tool_call.id.clone(), result)
                    );

                    working_context.push(result_msg.clone());
                    let _ = tx.send(result_msg);
                }

                if iteration == max_iterations - 1 {
                    tracing::warn!(
                        "ToolAgent reached max iterations ({}), stopping",
                        max_iterations
                    );
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llm::{ChatPayload, ChatRequest, ChatChunk, ToolDefinition, ToolCall};
    use futures::stream;

    struct MockToolModel {
        // Returns tool call on first call, plain response on second
        call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait]
    impl ChatModel for MockToolModel {
        async fn chat(&self, _request: &ChatRequest) -> anyhow::Result<ChatMessage> {
            let count = self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            if count == 0 {
                // First call: return tool call
                Ok(ChatMessage::assistant(ChatPayload::with_tool_calls(
                    "Let me check that".to_string(),
                    vec![ToolCall {
                        id: "call_1".to_string(),
                        name: "test_tool".to_string(),
                        arguments: serde_json::json!({}),
                    }],
                )))
            } else {
                // Second call: return final response
                Ok(ChatMessage::assistant(ChatPayload::text("Done!")))
            }
        }

        async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>> {
            // For simplicity, just use chat and wrap in stream
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
    }

    impl ConversationContext for MockContext {
        fn iter(&self) -> impl Iterator<Item = &ChatMessage> {
            self.messages.iter()
        }

        fn len(&self) -> usize {
            self.messages.len()
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

        let context = MockContext {
            messages: vec![ChatMessage::user(ChatPayload::text("Hi"))],
        };

        let agent = ToolAgent::new(Arc::new(tools), 5);
        let messages = agent.execute(&context, Arc::new(model)).await.unwrap();

        // Should have: assistant with tool call, tool result, final assistant response
        assert!(messages.len() >= 3);

        // First message should have tool calls
        assert!(!messages[0].get_tool_calls().is_empty());

        // Second message should be tool result
        assert!(!messages[1].get_tool_results().is_empty());

        // Last message should be final response
        assert_eq!(messages.last().unwrap().get_text(), "Done!");
    }
}
