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
/// Makes one call to the model and returns the response.
/// This is the most basic agent - just forwards the context to the model.
///
/// # Example
///
/// ```ignore
/// use noema_core::{SimpleAgent, Agent};
///
/// let agent = SimpleAgent::new();
/// let messages = agent.execute(&context, &model).await?;
/// assert_eq!(messages.len(), 1); // One assistant response
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
        context: &(impl ConversationContext + Sync),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let request = ChatRequest::new(context.iter());
        let response = model.chat(&request).await?;

        Ok(vec![response])
    }

    async fn execute_stream(
        &self,
        context: &(impl ConversationContext + Sync),
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = ChatMessage> + Send>>> {
        let request = ChatRequest::new(context.iter());
        let stream = model.stream_chat(&request).await?;

        // Convert ChatChunk stream to ChatMessage stream
        let message_stream = stream.map(|chunk| ChatMessage::from(chunk));

        Ok(Box::pin(message_stream))
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
    }

    #[tokio::test]
    async fn test_simple_agent() {
        let model = Arc::new(MockModel {
            response: "Hello!".to_string(),
        });

        let context = MockContext {
            messages: vec![ChatMessage::user(ChatPayload::text("Hi"))],
        };

        let agent = SimpleAgent::new();
        let messages = agent.execute(&context, model).await.unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].get_text(), "Hello!");
    }

    #[tokio::test]
    async fn test_simple_agent_stream() {
        use futures::StreamExt;

        let model = Arc::new(MockModel {
            response: "Hello!".to_string(),
        });

        let context = MockContext {
            messages: vec![ChatMessage::user(ChatPayload::text("Hi"))],
        };

        let agent = SimpleAgent::new();
        let mut stream = agent.execute_stream(&context, model).await.unwrap();

        let mut messages = Vec::new();
        while let Some(msg) = stream.next().await {
            messages.push(msg);
        }

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].get_text(), "Hello!");
    }
}
