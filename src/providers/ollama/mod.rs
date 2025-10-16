use async_trait::async_trait;
use crate::{ChatChunk, ChatModel, ChatMessage, ModelProvider};
use reqwest;
mod api;
use api::{ChatRequest, ChatResponse, ListModelsResponse};
use futures::{stream::{self, Stream}, StreamExt};


pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn default() -> Self {
        Self::new("http://localhost:11434")
    }

    pub fn new(base_url: &str) -> Self {
        OllamaProvider {
            client: reqwest::Client::new(), 
            base_url: base_url.to_string(),
        }
    }
}

pub struct OllamaChatModel {
    client: reqwest::Client,
    base_url: String,
    model_name: String,
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
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

    fn create_chat_model(&self, model_name: &str) -> Option<impl ChatModel> {
        Some(OllamaChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        ))
    }
}

impl OllamaChatModel {
    pub fn new(client: reqwest::Client, base_url: String, model_name: String) -> Self {
        OllamaChatModel {
            client,
            base_url,
            model_name,
        }
    }
}

#[async_trait]
impl ChatModel for OllamaChatModel {
    async fn chat(&self, messages: Vec<ChatMessage>) -> anyhow::Result<ChatMessage> {
        let url = format!("{}/api/chat", self.base_url);

        let ollama_messages: Vec<_> = messages
            .into_iter()
            .map(|msg: ChatMessage| msg.into())
            .collect();
        
        let request = ChatRequest {
            model: self.model_name.clone(),
            messages: ollama_messages,
            stream: Some(false),
        };

        let response = self.client.post(&url).json(&request).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Request failed with status: {}", response.status()));
        }

        let message = response.json::<ChatResponse>().await?;
        Ok(message.message.into())
    }


    async fn stream_chat(&self, messages: Vec<ChatMessage>) -> anyhow::Result<impl Stream<Item = ChatChunk>> {
        let url = format!("{}/api/chat", self.base_url);

        let ollama_messages: Vec<_> = messages
            .into_iter()
            .map(|msg: ChatMessage| msg.into())
            .collect();
        
        let request = ChatRequest {
            model: self.model_name.clone(),
            messages: ollama_messages,
            stream: Some(true),
        };

        let response = self.client.post(&url).json(&request).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Request failed with status: {}", response.status()));
        }

        let bytes = response.bytes_stream();
        Ok(bytes
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
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| {
                    match serde_json::from_str::<ChatResponse>(line) {
                        Ok(chat_response) => Some(chat_response.message.into()),
                        Err(e) => {
                            eprintln!("Failed to parse chunk: {}: {}", line, e);
                            None
                        }
                    }
                })
                .collect();
            stream::iter(messages)
        }))
    }
}