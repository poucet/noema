use super::api::{OllamaRequest, OllamaResponse};
use crate::client::Client;
use crate::{ChatMessage, ChatModel, ChatRequest, ChatStream};
use async_trait::async_trait;
use futures::StreamExt;

pub struct OllamaChatModel {
    client: Client,
    base_url: String,
    model_name: String,
}

impl OllamaChatModel {
    pub fn new(client: Client, base_url: String, model_name: String) -> Self {
        OllamaChatModel {
            client,
            base_url,
            model_name,
        }
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
        let streamed_response = self.client.post_stream(url, &request, |m| Some(m)).await?;
        Ok(Box::pin(
            streamed_response.map(|chunk: OllamaResponse| chunk.into()),
        ))
    }
}
