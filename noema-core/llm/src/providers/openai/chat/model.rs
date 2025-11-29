use crate::api::{ChatChunk, ChatMessage, ChatRequest, Role};
use crate::client::Client;
use crate::ChatModel;
use crate::ChatStream;
use async_trait::async_trait;
use futures::StreamExt;

use super::api::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};

#[derive(Clone)]
pub struct OpenAIChatModel {
    client: Client,
    base_url: String,
    model_name: String,
}

impl OpenAIChatModel {
    pub fn new(client: Client, base_url: String, model_name: String) -> Self {
        OpenAIChatModel {
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
impl ChatModel for OpenAIChatModel {
    fn name(&self) -> &str {
        &self.model_name
    }

    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatMessage> {
        let openai_request =
            ChatCompletionRequest::from_request(self.model_name.clone(), request, false);
        let response: ChatCompletionResponse =
            self.client.post(self.chat_url(), &openai_request).await?;
        Ok(response.into())
    }

    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let openai_request =
            ChatCompletionRequest::from_request(self.model_name.clone(), request, true);

        let stream = self
            .client
            .post_stream::<_, _, _, ChatCompletionChunk>(self.chat_url(), &openai_request, |m| {
                // OpenAI streaming format uses SSE with "data: " prefix
                let trimmed = m.trim();
                if trimmed.starts_with("data: ") {
                    let json_str = trimmed.strip_prefix("data: ").unwrap();
                    // OpenAI sends "data: [DONE]" as the final message
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
