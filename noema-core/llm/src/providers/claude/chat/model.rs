use crate::client::Client;
use crate::traffic_log;

use super::api::{Delta, MessagesRequest, MessagesResponse, StreamEvent};
use crate::{ChatMessage, ChatModel, ChatRequest, ChatStream};
use async_trait::async_trait;
use futures::StreamExt;
use tracing::warn;

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
    fn name(&self) -> &str {
        &self.model_name
    }

    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatMessage> {
        let url = format!("{}/messages", self.base_url);

        let api_request = MessagesRequest::from_chat_request(&self.model_name, request, false);
        traffic_log::log_request(&self.model_name, &api_request);

        match self.client.post(url, &api_request).await {
            Ok(response) => {
                traffic_log::log_response(&self.model_name, &response);
                let response: MessagesResponse = response;
                Ok(response.into())
            }
            Err(e) => {
                traffic_log::log_error(&self.model_name, &e.to_string());
                Err(e)
            }
        }
    }

    async fn stream_chat(&self, request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let url = format!("{}/messages", self.base_url);

        let api_request = MessagesRequest::from_chat_request(&self.model_name, request, true);
        traffic_log::log_stream_start(&self.model_name, &api_request);

        let streamed_response = self
            .client
            .post_stream(url, &api_request, |line: &str| line.strip_prefix("data: "))
            .await?;

        // Process Claude's streaming events and extract text deltas
        let chunk_stream = streamed_response.filter_map(|event: StreamEvent| {
            async move {
                match event {
                    StreamEvent::ContentBlockDelta { delta, .. } => match delta {
                        Delta::TextDelta { text } => Some(crate::ChatChunk::assistant(crate::ChatPayload::text(text))),
                        Delta::ThinkingDelta { thinking } => Some(crate::ChatChunk::assistant(crate::ChatPayload::text(thinking))), // Represent thinking as text
                        Delta::InputJsonDelta { partial_json } => {
                            // For tool use, we could accumulate and return, but for now skip
                            warn!("Skipping tool use JSON delta: {}", partial_json);
                            None
                        }
                    },
                    StreamEvent::Error { error } => {
                        warn!(
                            "Received error event: {} - {}",
                            error.error_type, error.message
                        );
                        None
                    }
                    // Ignore other event types
                    _ => None,
                }
            }
        });

        Ok(Box::pin(chunk_stream))
    }
}
