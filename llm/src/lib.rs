use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;
use std::sync::Arc;

pub mod api;
mod client;
pub mod providers;
pub mod tools;
pub use api::*;
pub use tools::ToolRegistry;

pub type ChatStream = Pin<Box<dyn Stream<Item = ChatChunk> + Send>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelCapability {
    Text,
    Embedding,
    Image,
}

#[derive(Clone, Debug)]
pub struct ModelDefinition {
    pub id: String,
    pub capabilities: Vec<ModelCapability>,
}

impl ModelDefinition {
    pub fn new(id: impl Into<String>, capabilities: Vec<ModelCapability>) -> Self {
        Self {
            id: id.into(),
            capabilities,
        }
    }

    pub fn text_model(id: impl Into<String>) -> Self {
        Self::new(id, vec![ModelCapability::Text])
    }

    pub fn has_capability(&self, capability: &ModelCapability) -> bool {
        self.capabilities.contains(capability)
    }
}

#[async_trait]
pub trait ChatModel {
    async fn chat(&self, messages: &ChatRequest) -> anyhow::Result<ChatMessage>;

    async fn stream_chat(&self, messages: &ChatRequest) -> anyhow::Result<ChatStream>;
}

// Blanket implementation for Arc<dyn ChatModel> to make it easier to work with
#[async_trait]
impl ChatModel for Arc<dyn ChatModel + Send + Sync> {
    async fn chat(&self, messages: &ChatRequest) -> anyhow::Result<ChatMessage> {
        (**self).chat(messages).await
    }

    async fn stream_chat(&self, messages: &ChatRequest) -> anyhow::Result<ChatStream> {
        (**self).stream_chat(messages).await
    }
}

#[async_trait]
pub trait ModelProvider {
    /// List available models from the provider with their capabilities
    async fn list_models(&self) -> anyhow::Result<Vec<ModelDefinition>>;

    /// Create a chat model by name, returned as Arc for sharing across threads
    fn create_chat_model(&self, model_name: &str) -> Option<Arc<dyn ChatModel + Send + Sync>>;
}
