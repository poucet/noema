use super::api::{GenerateContentRequest, GenerateContentResponse};
use crate::client::Client;
use crate::{ChatMessage, ChatModel, ChatRequest, ChatStream};
use async_trait::async_trait;
use futures::StreamExt;

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
impl ChatModel for GeminiChatModel {
    fn name(&self) -> &str {
        &self.model_name
    }

    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatMessage> {
        let url = format!("{}/{}:generateContent", self.base_url, self.model_name);

        let request: GenerateContentRequest = GenerateContentRequest::from(request);
        let response: GenerateContentResponse = self.client.post(url, &request).await?;
        Ok(response.into())
    }
    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let url = format!(
            "{}/{}:streamGenerateContent?alt=sse",
            self.base_url, self.model_name
        );

        let request: GenerateContentRequest = GenerateContentRequest::from(request);
        let streamed_response = self
            .client
            .post_stream(url, &request, |line: &str| line.strip_prefix("data: "))
            .await?;
        Ok(Box::pin(
            streamed_response.map(|chunk: GenerateContentResponse| chunk.into()),
        ))
    }
}
