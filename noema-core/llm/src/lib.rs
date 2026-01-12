use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;
use std::sync::Arc;

pub mod api;
mod client;
pub mod providers;
pub mod registry;
pub mod tools;
pub mod traffic_log;
pub use api::*;
pub use providers::GeneralModelProvider;
pub use registry::{
    create_model, get_provider_info, list_all_models, list_models, list_providers, ModelId,
    ModelInfo, ProviderInfo,
};
pub use tools::ToolRegistry;

pub type ChatStream = Pin<Box<dyn Stream<Item = ChatChunk> + Send>>;

/// Capabilities and characteristics of a model.
///
/// These are used to:
/// - Filter models by what they can do (e.g., only show models that support vision)
/// - Display capability indicators in the UI
/// - Enforce privacy rules (e.g., block cloud models for private content)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModelCapability {
    // === Content Processing ===
    /// Can process and generate text (chat/completion)
    Text,
    /// Can process/understand images (vision)
    Vision,
    /// Can process audio input
    AudioInput,
    /// Can generate embeddings
    Embedding,

    // === Generation ===
    /// Can generate images
    ImageGeneration,
    /// Can generate audio (text-to-speech)
    AudioGeneration,

    // === Advanced Features ===
    /// Supports tool/function calling
    Tools,
    /// Supports extended thinking/reasoning (e.g., Claude's thinking, o1's reasoning)
    Thinking,
    /// Supports streaming responses
    Streaming,

    // === Privacy ===
    /// Data stays private - never leaves the device (local models)
    Private,
}

#[derive(Clone, Debug)]
pub struct ModelDefinition {
    pub id: String,
    pub display_name: Option<String>,
    pub capabilities: Vec<ModelCapability>,
    pub context_window: Option<u32>,
}

impl ModelDefinition {
    pub fn new(id: impl Into<String>, capabilities: Vec<ModelCapability>) -> Self {
        Self {
            id: id.into(),
            display_name: None,
            capabilities,
            context_window: None,
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
            context_window: None,
        }
    }

    pub fn with_context_window(mut self, context_window: u32) -> Self {
        self.context_window = Some(context_window);
        self
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
    /// Model ID (e.g., "claude-sonnet-4-20250514")
    fn id(&self) -> &str;

    /// Display name for the model
    fn name(&self) -> &str;

    async fn chat(&self, messages: &ChatRequest) -> anyhow::Result<ChatMessage>;

    async fn stream_chat(&self, messages: &ChatRequest) -> anyhow::Result<ChatStream>;
}

// Blanket implementation for Arc<dyn ChatModel> to make it easier to work with
#[async_trait]
impl ChatModel for Arc<dyn ChatModel + Send + Sync> {
    fn id(&self) -> &str {
        (**self).id()
    }

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

