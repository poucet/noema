use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use futures::stream::Stream;
pub mod providers;


#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[async_trait]
pub trait ChatModel {
    async fn chat(&self, messages: Vec<ChatMessage>) -> anyhow::Result<ChatMessage>;

    async fn stream_chat(&self, messages: Vec<ChatMessage>) -> anyhow::Result<impl Stream<Item = ChatMessage>>;
}

#[async_trait]
pub trait ModelProvider {
    // List available models from the provider.
    async fn list_models(&self) -> anyhow::Result<Vec<String>>;

    // Get a specific model by name.
    fn create_chat_model(&self, model_name: &str) -> Option<impl ChatModel>;
}