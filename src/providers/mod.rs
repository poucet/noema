pub (crate) mod claude;
pub (crate) mod gemini;
pub (crate) mod ollama;

use async_trait::async_trait;
use crate::{ChatModel, ChatRequest, ChatStream, ModelProvider};

pub use claude::{ClaudeChatModel, ClaudeProvider};
pub use ollama::{OllamaChatModel, OllamaProvider};
pub use gemini::{GeminiChatModel, GeminiProvider};

pub enum GeneralModelProvider {
    Ollama(OllamaProvider),
    Gemini(GeminiProvider),
    Claude(ClaudeProvider)
}

pub enum GeneralChatModel {
    Ollama(OllamaChatModel),
    Gemini(GeminiChatModel),
    Claude(ClaudeChatModel)
}

#[async_trait]
impl ModelProvider for GeneralModelProvider {   
    type ModelType = GeneralChatModel;

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        match self {
            GeneralModelProvider::Ollama(provider) => provider.list_models().await,
            GeneralModelProvider::Gemini(provider) => provider.list_models().await,
            GeneralModelProvider::Claude(provider) => provider.list_models().await,
        }
    }

    fn create_chat_model(&self, model_name: &str) -> Option<GeneralChatModel> {
        match self {
            GeneralModelProvider::Ollama(provider) => {
                provider.create_chat_model(model_name).map(GeneralChatModel::Ollama)
            },
            GeneralModelProvider::Gemini(provider) => {
                provider.create_chat_model(model_name).map(GeneralChatModel::Gemini)
            },
            GeneralModelProvider::Claude(provider) => {
                provider.create_chat_model(model_name).map(GeneralChatModel::Claude)
            },
        }
    }
}

#[async_trait]
impl ChatModel for GeneralChatModel {
    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<crate::ChatMessage> {
        match self {
            GeneralChatModel::Ollama(model) => model.chat(request).await,
            GeneralChatModel::Gemini(model) => model.chat(request).await,
            GeneralChatModel::Claude(model) => model.chat(request).await,
        }
    }

    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        match self {
            GeneralChatModel::Ollama(model) => model.stream_chat(request).await,
            GeneralChatModel::Gemini(model) => model.stream_chat(request).await,
            GeneralChatModel::Claude(model) => model.stream_chat(request).await,
        }
    }
}