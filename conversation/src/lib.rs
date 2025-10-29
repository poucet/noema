use anyhow::Result;
use futures::stream::Stream;
use llm::api::{ChatChunk, ChatMessage, ChatPayload, ChatRequest};
use llm::{ChatModel, ChatStream};
use std::pin::Pin;
use std::task::{Context, Poll};

/// Trait for types that can be accumulated (monoidal append operation)
///
/// This trait represents a monoid-like structure where values can be combined
/// via the `append` operation with an identity element provided by `Default`.
pub trait Accumulate: Default + Clone {
    /// Append another value to this one (monoidal operation)
    fn append(&mut self, other: Self);
}

impl Accumulate for ChatPayload {
    fn append(&mut self, other: Self) {
        self.content.extend(other.content);
    }
}

impl Accumulate for ChatMessage {
    fn append(&mut self, other: Self) {
        self.payload.append(other.payload);
    }
}
/// A generic stream wrapper that accumulates values from stream items
///
/// Generic over:
/// - S: The inner stream type
/// - T: The stream item type (must convert Into<U>)
/// - U: The accumulator type (must be Accumulate)
pub struct AccumulatingStream<S, T, U>
where
    S: Stream<Item = T> + Unpin,
    T: Into<U>,
    U: Accumulate,
{
    inner: S,
    accumulated: U,
    _phantom: std::marker::PhantomData<T>,
}

impl<S, T, U> AccumulatingStream<S, T, U>
where
    S: Stream<Item = T> + Unpin,
    T: Into<U>,
    U: Accumulate,
{
    fn new(stream: S) -> Self {
        Self {
            inner: stream,
            accumulated: U::default(),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get a reference to the accumulated value
    pub fn accumulated(&self) -> &U {
        &self.accumulated
    }

    /// Consume the stream and return the accumulated value
    pub fn into_accumulated(self) -> U {
        self.accumulated
    }
}

impl<S, T, U> Stream for AccumulatingStream<S, T, U>
where
    S: Stream<Item = T> + Unpin,
    T: Into<U> + Clone,
    U: Accumulate,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(item)) => {
                let converted: U = item.clone().into();
                this.accumulated.append(converted);
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S, T, U> Unpin for AccumulatingStream<S, T, U>
where
    S: Stream<Item = T> + Unpin,
    T: Into<U>,
    U: Accumulate,
{}

/// Type alias for accumulating ChatChunks into ChatMessage
pub type ChatAccumulatingStream = AccumulatingStream<ChatStream, ChatChunk, ChatMessage>;

/// A streaming response that automatically saves to conversation history when dropped
pub struct ConversationStream<'a> {
    stream: ChatAccumulatingStream,
    conversation: &'a mut Conversation,
    finalized: bool,
}

impl<'a> ConversationStream<'a> {
    fn new(stream: ChatAccumulatingStream, conversation: &'a mut Conversation) -> Self {
        Self {
            stream,
            conversation,
            finalized: false,
        }
    }

    /// Get a reference to the accumulated message so far
    pub fn accumulated(&self) -> &ChatMessage {
        self.stream.accumulated()
    }
}

impl<'a> Stream for ConversationStream<'a> {
    type Item = ChatChunk;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.stream).poll_next(cx)
    }
}

impl<'a> Drop for ConversationStream<'a> {
    fn drop(&mut self) {
        if !self.finalized {
            let accumulated = self.stream.accumulated().clone();
            if !accumulated.payload.content.is_empty() {
                self.conversation.history.push(accumulated);
            }
            self.finalized = true;
        }
    }
}

impl<'a> Unpin for ConversationStream<'a> {}

/// A conversation that maintains history and provides methods for interacting with an LLM
pub struct Conversation {
    model: Box<dyn ChatModel + Send>,
    history: Vec<ChatMessage>,
}

impl Conversation {
    /// Create a new conversation with the given chat model
    pub fn new(model: impl ChatModel + Send + 'static) -> Self {
        Self {
            model: Box::new(model),
            history: Vec::new(),
        }
    }

    /// Create a new conversation with an initial system message
    pub fn with_system_message(model: impl ChatModel + Send + 'static, system_message: impl Into<ChatPayload>) -> Self {
        let mut conversation = Self::new(model);
        conversation.add_system_message(system_message);
        conversation
    }

    /// Replace the current model with a new one
    pub fn set_model(&mut self, model: impl ChatModel + Send + 'static) {
        self.model = Box::new(model);
    }

    /// Add a system message to the conversation history
    pub fn add_system_message(&mut self, payload: impl Into<ChatPayload>) {
        self.history.push(ChatMessage::system(payload.into()));
    }

    /// Add a user message to the conversation history
    pub fn add_user_message(&mut self, payload: impl Into<ChatPayload>) {
        self.history.push(ChatMessage::user(payload.into()));
    }

    /// Add an assistant message to the conversation history
    pub fn add_assistant_message(&mut self, payload: impl Into<ChatPayload>) {
        self.history.push(ChatMessage::assistant(payload.into()));
    }

    /// Send a user message and get a response, updating the conversation history
    pub async fn send(&mut self, message: impl Into<ChatPayload>) -> Result<ChatMessage > {
        self.add_user_message(message);

        let request = ChatRequest::new(self.history.clone());
        let response = self.model.chat(&request).await?;

        self.history.push(response.clone());

        Ok(response)
    }

    /// Send a user message and get a streaming response that automatically saves to history
    ///
    /// The returned stream automatically saves the accumulated response to conversation
    /// history when it is dropped (goes out of scope).
    ///
    /// # Example
    /// ```no_run
    /// # use conversation::Conversation;
    /// # async fn example(mut conv: Conversation) -> anyhow::Result<()> {
    /// use futures::StreamExt;
    ///
    /// let mut stream = conv.send_stream("Hello!").await?;
    /// while let Some(chunk) = stream.next().await {
    ///     print!("{}", chunk.get_text());
    /// }
    /// // History is automatically updated when stream is dropped here
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_stream(&mut self, message: impl Into<ChatPayload>) -> Result<ConversationStream<'_>> {
        self.add_user_message(message);

        let request = ChatRequest::new(self.history.clone());
        let stream = self.model.stream_chat(&request).await?;
        let acc_stream = ChatAccumulatingStream::new(stream);

        Ok(ConversationStream::new(acc_stream, self))
    }

    /// Get a reference to the conversation history
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    /// Get a mutable reference to the conversation history
    pub fn history_mut(&mut self) -> &mut Vec<ChatMessage> {
        &mut self.history
    }

    /// Clear the conversation history
    pub fn clear(&mut self) {
        self.history.clear();
    }

    /// Get the number of messages in the conversation history
    pub fn message_count(&self) -> usize {
        self.history.len()
    }

    /// Remove the last message from the conversation history
    pub fn pop_last_message(&mut self) -> Option<ChatMessage> {
        self.history.pop()
    }

    /// Get a reference to the underlying model
    pub fn model(&self) -> &(dyn ChatModel + Send) {
        &*self.model
    }

    /// Get a mutable reference to the underlying model
    pub fn model_mut(&mut self) -> &mut (dyn ChatModel + Send) {
        &mut *self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llm::api::{ChatMessage, ChatRequest, Role};
    use llm::{ChatModel, ChatStream};
    use async_trait::async_trait;
    use futures::stream;

    struct MockChatModel {
        response: String,
    }

    #[async_trait]
    impl ChatModel for MockChatModel {
        async fn chat(&self, _request: &ChatRequest) -> Result<ChatMessage> {
            Ok(ChatMessage::assistant(ChatPayload::text(self.response.clone())))
        }

        async fn stream_chat(&self, _request: &ChatRequest) -> Result<ChatStream> {
            let chunk = ChatChunk::assistant(ChatPayload::text(self.response.clone()));
            Ok(Box::pin(stream::iter(vec![chunk])))
        }
    }

    #[tokio::test]
    async fn test_new_conversation() {
        let model = MockChatModel {
            response: "Hello!".to_string(),
        };
        let conversation = Conversation::new(model);
        assert_eq!(conversation.message_count(), 0);
    }

    #[tokio::test]
    async fn test_with_system_message() {
        let model = MockChatModel {
            response: "Hello!".to_string(),
        };
        let conversation = Conversation::with_system_message(model, "You are a helpful assistant.");
        assert_eq!(conversation.message_count(), 1);
        assert!(matches!(conversation.history()[0].role, Role::System));
    }

    #[tokio::test]
    async fn test_send() {
        let model = MockChatModel {
            response: "Hello!".to_string(),
        };
        let mut conversation = Conversation::new(model);

        let response = conversation.send("Hi there!").await.unwrap();
        assert_eq!(response.get_text(), "Hello!");
        assert_eq!(conversation.message_count(), 2);
    }

    #[tokio::test]
    async fn test_clear() {
        let model = MockChatModel {
            response: "Hello!".to_string(),
        };
        let mut conversation = Conversation::new(model);
        conversation.add_user_message("Test");
        assert_eq!(conversation.message_count(), 1);

        conversation.clear();
        assert_eq!(conversation.message_count(), 0);
    }

    #[tokio::test]
    async fn test_pop_last_message() {
        let model = MockChatModel {
            response: "Hello!".to_string(),
        };
        let mut conversation = Conversation::new(model);
        conversation.add_user_message("Test");
        assert_eq!(conversation.message_count(), 1);

        let popped = conversation.pop_last_message();
        assert!(popped.is_some());
        assert_eq!(conversation.message_count(), 0);
    }

    #[tokio::test]
    async fn test_send_stream_accumulates() {
        use futures::StreamExt;

        let model = MockChatModel {
            response: "Hello!".to_string(),
        };
        let mut conversation = Conversation::new(model);

        {
            let mut stream = conversation.send_stream("Hi there!").await.unwrap();

            // Consume the stream
            let mut chunks = Vec::new();
            while let Some(chunk) = stream.next().await {
                chunks.push(chunk.get_text());
            }

            // Stream should have accumulated the content
            assert_eq!(stream.accumulated().get_text(), "Hello!");

            // Stream is automatically finalized when it goes out of scope here
        }

        // Should have user message + assistant response
        assert_eq!(conversation.message_count(), 2);
        assert_eq!(conversation.history()[1].get_text(), "Hello!");
    }

}
