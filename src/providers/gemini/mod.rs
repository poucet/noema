use async_trait::async_trait;
use crate::{ChatRequest, ChatChunk, ChatStream, ChatMessage, ChatModel, ModelProvider};
use reqwest::{self, header};
mod api;
use api::{GenerateContentRequest, GenerateContentResponse, ListModelsResponse};
use futures::{stream::{self}, StreamExt};


pub struct GeminiProvider {
    client: reqwest::Client,
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
            client: reqwest::Client::builder().default_headers(headers).build().unwrap(),
            base_url: base_url.to_string(),
        }
    }
}

pub struct GeminiChatModel {
    client: reqwest::Client,
    base_url: String,
    model_name: String,
}

impl GeminiChatModel {
    pub fn new(client: reqwest::Client, base_url: String, model_name: String) -> Self {
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
        let resp = self.client.get(&url).send().await;

        match resp {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<ListModelsResponse>().await {
                        Ok(models) => Ok(models.models.iter().map(|m| m.name.clone()).collect()),
                        Err(_) => Err(anyhow::anyhow!("Failed to parse response")),
                    }
                } else {
                    Err(anyhow::anyhow!("Request failed with status: {}", response.status()))
                }
            }
            Err(_) => Err(anyhow::anyhow!("Request error")),
        }
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
        let request = GenerateContentRequest::from(request);
        let response = self.client
            .post(&url)
            .json(&request).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Request failed with status: {}", response.status()));
        }

        let message = response.json::<GenerateContentResponse>().await?;
        Ok(message.into())
    }

    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let url = format!("{}/{}:streamGenerateContent?alt=sse", self.base_url, self.model_name);
        let request = GenerateContentRequest::from(request);
        let response = self.client
            .post(&url)
            .json(&request).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Request failed with status: {}", response.status()));
        }

        let bytes = response.bytes_stream();
        Ok(Box::pin(bytes
        .flat_map(|chunk| {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading chunk: {}", e);
                    return stream::iter(vec![]);
                }
            };
            let chunk_str = String::from_utf8_lossy(&chunk);
            let messages: Vec<ChatChunk> = chunk_str
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| {
                    match serde_json::from_str::<GenerateContentResponse>(line) {
                        Ok(chat_response) => Some(chat_response.into()),
                        Err(e) => {
                            eprintln!("Failed to parse chunk: {}: {}", line, e);
                            None
                        }
                    }
                })
                .collect();
            stream::iter(messages)
        })))
    }
}