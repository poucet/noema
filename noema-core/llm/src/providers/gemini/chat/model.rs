use super::api::{GenerateContentRequest, GenerateContentResponse};
use crate::client::Client;
use crate::traffic_log;
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

        let api_request: GenerateContentRequest = GenerateContentRequest::from(request);
        traffic_log::log_request(&self.model_name, &api_request);

        match self.client.post(url, &api_request).await {
            Ok(response) => {
                traffic_log::log_response(&self.model_name, &response);
                let response: GenerateContentResponse = response;
                Ok(response.into())
            }
            Err(e) => {
                traffic_log::log_error(&self.model_name, &e.to_string());
                Err(e)
            }
        }
    }

    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let url = format!(
            "{}/{}:streamGenerateContent?alt=sse",
            self.base_url, self.model_name
        );

        let api_request: GenerateContentRequest = GenerateContentRequest::from(request);
        traffic_log::log_stream_start(&self.model_name, &api_request);

        let streamed_response = self
            .client
            .post_stream(url, &api_request, |line: &str| line.strip_prefix("data: "))
            .await?;
        Ok(Box::pin(
            streamed_response.map(|chunk: GenerateContentResponse| chunk.into()),
        ))
    }
}
