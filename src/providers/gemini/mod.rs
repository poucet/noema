use async_trait::async_trait;
use crate::{ChatRequest, ChatStream, ChatMessage, ChatModel, ModelProvider};
use reqwest::{self, header};
mod api;
use api::{GenerateContentRequest, GenerateContentResponse, ListModelsResponse};
use futures::StreamExt;
use crate::client::Client;


pub struct GeminiProvider {
    client: Client,
    base_url: String,
}

impl GeminiProvider {
    pub fn default(api_key: &str) -> Self {
        Self::new("https://generativelanguage.googleapis.com/v1beta", api_key)
    }

    pub fn new(base_url: &str, api_key: &str) -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("x-goog-api-key", api_key.parse().unwrap());
        GeminiProvider {
            client: Client::with_headers(headers),
            base_url: base_url.to_string(),
        }
    }
}

pub struct GeminiChatModel {
    client: Client,
    base_url: String,
    model_name: String,
}

impl GeminiChatModel {
    pub fn new(client: Client, base_url: String, model_name: String) -> Self {
        GeminiChatModel {
            client,
            base_url,
            model_name,
        }
    }
}

#[async_trait]
impl ModelProvider for GeminiProvider {
    type ModelType = GeminiChatModel;

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/models", self.base_url);
        let response: ListModelsResponse = self.client.get(&url).await?;
        Ok(response.models.iter().map(|m| m.name.clone()).collect())
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Self::ModelType> {
        Some(GeminiChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        ))
    }
}

#[async_trait]
impl ChatModel for GeminiChatModel {
    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatMessage> {
        let url = format!("{}/{}:generateContent", self.base_url, self.model_name);
        
        let request: GenerateContentRequest = GenerateContentRequest::from(request);
        let response: GenerateContentResponse = self.client.post(url, &request).await?;
        Ok(response.into())
    }
    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let url = format!("{}/{}:streamGenerateContent?alt=sse", self.base_url, self.model_name);

        let request: GenerateContentRequest = GenerateContentRequest::from(request);
        let streamed_response = self.client
            .post_stream(url, request, |line: &str| line.strip_prefix("data: ")).await?;
        Ok(Box::pin(streamed_response.map(|chunk: GenerateContentResponse| chunk.into())))
    }
}