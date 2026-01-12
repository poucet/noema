//! Simple single-turn agent implementation

use crate::Agent;
use crate::ConversationContext;
use anyhow::Result;
use async_trait::async_trait;
use futures::stream::StreamExt;
use llm::{ChatMessage, ChatModel, ChatRequest};
use std::sync::Arc;

/// Simple single-turn agent
///
/// Makes one call to the model and adds the response to the context.
pub struct SimpleAgent;

impl SimpleAgent {
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
        context: &mut dyn ConversationContext,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> Result<()> {
        let messages = context.messages().await?;
        let request = ChatRequest::new(messages.iter());
        let response = model.chat(&request).await?;

        context.add(response);
        Ok(())
    }

    async fn execute_stream(
        &self,
        context: &mut dyn ConversationContext,
        model: Arc<dyn ChatModel + Send + Sync>,
    ) -> Result<()> {
        let messages = context.messages().await?;
        let request = ChatRequest::new(messages.iter());
        let mut stream = model.stream_chat(&request).await?;

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
    use crate::MessagesGuard;
    use llm::{ChatChunk, ChatPayload, ChatRequest};
    use futures::stream;
    use std::pin::Pin;
    use futures::stream::Stream;

    struct MockModel {
        response: String,
    }

    #[async_trait]
    impl ChatModel for MockModel {
        fn name(&self) -> &str {
            "mock"
        }

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

    #[tokio::test]
    async fn test_simple_agent() {
        let model = Arc::new(MockModel {
            response: "Hello!".to_string(),
        });

        let mut context = MockContext {
            messages: vec![ChatMessage::user(ChatPayload::text("Hi"))],
            pending: vec![],
        };

        let agent = SimpleAgent::new();
        agent.execute(&mut context, model).await.unwrap();

        assert_eq!(context.pending.len(), 1);
        assert_eq!(context.pending[0].get_text(), "Hello!");
    }

    #[tokio::test]
    async fn test_simple_agent_stream() {
        let model = Arc::new(MockModel {
            response: "Hello!".to_string(),
        });

        let mut context = MockContext {
            messages: vec![ChatMessage::user(ChatPayload::text("Hi"))],
            pending: vec![],
        };

        let agent = SimpleAgent::new();
        agent.execute_stream(&mut context, model).await.unwrap();

        assert_eq!(context.pending.len(), 1);
        assert_eq!(context.pending[0].get_text(), "Hello!");
    }
}
