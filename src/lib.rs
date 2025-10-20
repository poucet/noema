use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

mod client;
pub mod providers;

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatChunk {
    pub role: Role,
    pub content: String,
}

pub type ChatStream = Pin<Box<dyn Stream<Item = ChatChunk> + Send>>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatRequest {
    messages: Vec<ChatMessage>,
}

impl ChatRequest {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        ChatRequest { messages }
    }
}

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
