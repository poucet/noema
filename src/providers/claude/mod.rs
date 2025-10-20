mod api;

use async_trait::async_trait;
use crate::{ChatRequest, ChatStream, ChatMessage, ChatModel, ModelProvider};
use reqwest::{self, header};
use api::{MessagesRequest, MessagesResponse, ListModelsResponse};
use futures::StreamExt;
use crate::client::Client;

pub struct ClaudeProvider {
    client: Client,
    base_url: String,
}

impl ClaudeProvider {
    pub fn default(api_key: &str) -> Self {
        Self::new("https://api.anthropic.com/v1", api_key)
    }

    pub fn new(base_url: &str, api_key: &str) -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("x-api-key", api_key.parse().unwrap());
        headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
        ClaudeProvider {
            client: Client::with_headers(headers),
            base_url: base_url.to_string(),
        }
    }
}

pub struct ClaudeChatModel {
    client: Client,
    base_url: String,
    model_name: String,
}

impl ClaudeChatModel {
    pub fn new(client: Client, base_url: String, model_name: String) -> Self {
        ClaudeChatModel {
            client,
            base_url,
            model_name,
        }
    }
}

#[async_trait]
impl ModelProvider for ClaudeProvider {
    type ModelType = ClaudeChatModel;

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        // TODO: Add support for pagination.
        let url = format!("{}/models", self.base_url);
        let response: ListModelsResponse = self.client.get(&url).await?;
        Ok(response.data.iter().map(|m| m.id.clone()).collect())
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Self::ModelType> {
        Some(ClaudeChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        ))
    }
}

#[async_trait]
impl ChatModel for ClaudeChatModel {
    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatMessage> {
        let url = format!("{}/messages", self.base_url);
        
        let request = MessagesRequest::from_chat_request(&self.model_name, request, false);
        // Print request as JSON
        println!("{:} for url {:}" , serde_json::json!(request), url);
        let response: MessagesResponse = self.client.post(url, &request).await?;
        Ok(response.into())
    }

    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let url = format!("{}/messages", self.base_url);

        let request = MessagesRequest::from_chat_request(&self.model_name, request, true);
        let streamed_response = self.client
            .post_stream(url, &request, |line: &str| line.strip_prefix("data: ")).await?;
        Ok(Box::pin(streamed_response.map(|chunk: MessagesResponse| chunk.into())))
    }
}