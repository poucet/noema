use async_trait::async_trait;
use futures::FutureExt;
use crate::{ChatRequest, ChatChunk, ChatModel, ChatMessage, ChatStream, ModelProvider};
use crate::client::Client;
mod api;
use api::{OllamaRequest, OllamaResponse, ListModelsResponse};
use futures::{StreamExt};


pub struct OllamaProvider {
    client: Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn default() -> Self {
        Self::new("http://localhost:11434")
    }

    pub fn new(base_url: &str) -> Self {
        OllamaProvider {
            client: Client::default(), 
            base_url: base_url.to_string(),
        }
    }
}

pub struct OllamaChatModel {
    client: Client,
    base_url: String,
    model_name: String,
}

impl OllamaChatModel {
    fn new(client: Client, base_url: String, model_name: String) -> Self {
        OllamaChatModel {
            client,
            base_url,
            model_name,
        }
    }
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    type ModelType = OllamaChatModel;

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let response: ListModelsResponse = self.client.get(&url).await?;
        Ok(response.models.iter().map(|m| m.name.clone()).collect())
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Self::ModelType> {
        Some(OllamaChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        ))
    }
}

#[async_trait]
impl ChatModel for OllamaChatModel {
    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatMessage> {
        let url = format!("{}/api/chat", self.base_url);

        let request = OllamaRequest::from_chat_request(&self.model_name, request, false);
        let response: OllamaResponse = self.client.post(url, &request).await?;
        Ok(response.into())
    }

    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let url = format!("{}/api/chat", self.base_url);

        let request = OllamaRequest::from_chat_request(&self.model_name, request, true);
        let streamed_response = self.client
            .post_stream(url, request, |m| Some(m)).await?;
        Ok(Box::pin(streamed_response.map(|chunk: OllamaResponse| chunk.into())))
    }
}