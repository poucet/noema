//! Simple single-turn agent implementation

use crate::Agent;
use crate::ConversationContext;
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use llm::{ChatMessage, ChatModel, ChatRequest};
use std::pin::Pin;
use std::sync::Arc;

/// Simple single-turn agent
///
/// Makes one call to the model and adds the response to the context.
/// This is the most basic agent - just forwards the context to the model.
///
/// # Example
///
/// ```ignore
/// use noema_core::{SimpleAgent, Agent};
///
/// let agent = SimpleAgent::new();
/// agent.execute(&mut context, model).await?;
/// // Context now contains the assistant response
/// ```
pub struct SimpleAgent;

impl SimpleAgent {
    /// Create a new simple agent
    pub fn new() -> Self {
        Self
    }
}

impl Default for SimpleAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for SimpleAgent {
    async fn execute(
        &self,
        context: &mut (impl ConversationContext + Send),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<()> {
        let request = ChatRequest::new(context.iter());
        let response = model.chat(&request).await?;

        context.add(response);
        Ok(())
    }

    async fn execute_stream(
        &self,
        context: &mut (impl ConversationContext + Send),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<()> {
        let request = ChatRequest::new(context.iter());
        let mut stream = model.stream_chat(&request).await?;

        // Stream chunks and add each as a message to context
        while let Some(chunk) = stream.next().await {
            let message = ChatMessage::from(chunk);
            context.add(message);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llm::{ChatPayload, ChatRequest, ChatChunk};
    use futures::stream;

    struct MockModel {
        response: String,
    }

    #[async_trait]
    impl ChatModel for MockModel {
        async fn chat(&self, _request: &ChatRequest) -> anyhow::Result<ChatMessage> {
            Ok(ChatMessage::assistant(ChatPayload::text(&self.response)))
        }

        async fn stream_chat(&self, _request: &ChatRequest) -> anyhow::Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>> {
            let chunk = ChatChunk::assistant(ChatPayload::text(&self.response));
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

        fn add(&mut self, message: ChatMessage) {
            self.messages.push(message);
        }

        fn extend(&mut self, messages: impl IntoIterator<Item = ChatMessage>) {
            self.messages.extend(messages);
        }
    }

    #[tokio::test]
    async fn test_simple_agent() {
        let model = Arc::new(MockModel {
            response: "Hello!".to_string(),
        });

        let mut context = MockContext {
            messages: vec![ChatMessage::user(ChatPayload::text("Hi"))],
        };

        let agent = SimpleAgent::new();
        agent.execute(&mut context, model).await.unwrap();

        assert_eq!(context.len(), 2); // User + assistant
        assert_eq!(context.messages[1].get_text(), "Hello!");
    }

    #[tokio::test]
    async fn test_simple_agent_stream() {
        let model = Arc::new(MockModel {
            response: "Hello!".to_string(),
        });

        let mut context = MockContext {
            messages: vec![ChatMessage::user(ChatPayload::text("Hi"))],
        };

        let agent = SimpleAgent::new();
        agent.execute_stream(&mut context, model).await.unwrap();

        assert_eq!(context.len(), 2); // User + assistant chunk
        assert_eq!(context.messages[1].get_text(), "Hello!");
    }
}
