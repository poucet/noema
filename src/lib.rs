use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;

pub mod api;
mod client;
pub mod providers;
pub use api::*;

pub type ChatStream = Pin<Box<dyn Stream<Item = ChatChunk> + Send>>;

#[async_trait]
pub trait ChatModel {
    async fn chat(&self, messages: &ChatRequest) -> anyhow::Result<ChatMessage>;

    async fn stream_chat(&self, messages: &ChatRequest) -> anyhow::Result<ChatStream>;
}

#[async_trait]
pub trait ModelProvider {
    type ModelType: ChatModel;
    // List available models from the provider.
    async fn list_models(&self) -> anyhow::Result<Vec<String>>;

    // Get a specific model by name.
    fn create_chat_model(&self, model_name: &str) -> Option<Self::ModelType>;
}
