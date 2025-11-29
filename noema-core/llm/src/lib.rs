use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;
use std::sync::Arc;

pub mod api;
mod client;
pub mod providers;
pub mod registry;
pub mod tools;
pub use api::*;
pub use providers::GeneralModelProvider;
pub use registry::{
    create_model, get_provider_info, list_all_models, list_models, list_providers, ModelId,
    ModelInfo, ProviderInfo,
};
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
    pub display_name: Option<String>,
    pub capabilities: Vec<ModelCapability>,
}

impl ModelDefinition {
    pub fn new(id: impl Into<String>, capabilities: Vec<ModelCapability>) -> Self {
        Self {
            id: id.into(),
            display_name: None,
            capabilities,
        }
    }

    pub fn with_display_name(
        id: impl Into<String>,
        display_name: impl Into<String>,
        capabilities: Vec<ModelCapability>,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: Some(display_name.into()),
            capabilities,
        }
    }

    pub fn text_model(id: impl Into<String>) -> Self {
        Self::new(id, vec![ModelCapability::Text])
    }

    /// Get the display name, falling back to id if not set
    pub fn name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.id)
    }

    pub fn has_capability(&self, capability: &ModelCapability) -> bool {
        self.capabilities.contains(capability)
    }
}

#[async_trait]
pub trait ChatModel {
    fn name(&self) -> &str;

    async fn chat(&self, messages: &ChatRequest) -> anyhow::Result<ChatMessage>;

    async fn stream_chat(&self, messages: &ChatRequest) -> anyhow::Result<ChatStream>;
}

// Blanket implementation for Arc<dyn ChatModel> to make it easier to work with
#[async_trait]
impl ChatModel for Arc<dyn ChatModel + Send + Sync> {
    fn name(&self) -> &str {
        (**self).name()
    }

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

