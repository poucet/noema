use crate::client::Client;

use super::api::{MessagesRequest, MessagesResponse};
use crate::{ChatMessage, ChatModel, ChatRequest, ChatStream};
use async_trait::async_trait;
use futures::StreamExt;

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
impl ChatModel for ClaudeChatModel {
    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatMessage> {
        let url = format!("{}/messages", self.base_url);

        let request = MessagesRequest::from_chat_request(&self.model_name, request, false);
        let response: MessagesResponse = self.client.post(url, &request).await?;
        Ok(response.into())
    }

    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let url = format!("{}/messages", self.base_url);

        let request = MessagesRequest::from_chat_request(&self.model_name, request, true);
        let _streamed_response = self
            .client
            .post_stream(url, &request, |line: &str| line.strip_prefix("data: "))
            .await?;
        todo!(
            "Make this work. Claude streaming API is very different from its regular API, unlike other providers."
        );

        #[allow(unreachable_code)]
        Ok(Box::pin(
            _streamed_response.map(|chunk: MessagesResponse| chunk.into()),
        ))
    }
}
