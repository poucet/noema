use crate::api::{ChatChunk, ChatMessage, ChatRequest, Role};
use crate::client::Client;
use crate::traffic_log;
use crate::ChatModel;
use crate::ChatStream;
use async_trait::async_trait;
use futures::StreamExt;

use super::api::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};

#[derive(Clone)]
pub struct MistralChatModel {
    client: Client,
    base_url: String,
    model_name: String,
}

impl MistralChatModel {
    pub fn new(client: Client, base_url: String, model_name: String) -> Self {
        MistralChatModel {
            client,
            base_url,
            model_name,
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

#[async_trait]
impl ChatModel for MistralChatModel {
    fn name(&self) -> &str {
        &self.model_name
    }

    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatMessage> {
        let mistral_request =
            ChatCompletionRequest::from_request(self.model_name.clone(), request, false);
        traffic_log::log_request(&self.model_name, &mistral_request);

        match self.client.post(self.chat_url(), &mistral_request).await {
            Ok(response) => {
                traffic_log::log_response(&self.model_name, &response);
                let response: ChatCompletionResponse = response;
                Ok(response.into())
            }
            Err(e) => {
                traffic_log::log_error(&self.model_name, &e.to_string());
                Err(e)
            }
        }
    }

    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let mistral_request =
            ChatCompletionRequest::from_request(self.model_name.clone(), request, true);
        traffic_log::log_stream_start(&self.model_name, &mistral_request);

        let stream = self
            .client
            .post_stream::<_, _, _, ChatCompletionChunk>(self.chat_url(), &mistral_request, |m| {
                // Mistral streaming format uses SSE with "data: " prefix
                let trimmed = m.trim();
                if trimmed.starts_with("data: ") {
                    let json_str = trimmed.strip_prefix("data: ").unwrap();
                    // Mistral sends "data: [DONE]" as the final message
                    if json_str == "[DONE]" {
                        return None;
                    }
                    Some(json_str)
                } else {
                    None
                }
            })
            .await?;

        let chat_stream = stream.map(|chunk| {
            let choice = &chunk.choices[0];
            let role = choice.delta.role.unwrap_or(Role::Assistant);
            let content = choice.delta.content.clone().unwrap_or_default();

            ChatChunk::new(role, crate::ChatPayload::text(content))
        });

        Ok(Box::pin(chat_stream))
    }
}
