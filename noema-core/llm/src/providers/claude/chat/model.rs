use crate::client::Client;
use crate::traffic_log;

use super::api::{ContentBlock, Delta, MessagesRequest, MessagesResponse, StreamEvent};
use crate::{ChatMessage, ChatModel, ChatRequest, ChatStream};
use async_trait::async_trait;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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
    fn id(&self) -> &str {
        &self.model_name
    }

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

        // Track tool calls being built up during streaming
        // Key: content block index, Value: (id, name, accumulated_json)
        let tool_calls: Arc<Mutex<HashMap<usize, (String, String, String)>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let tool_calls_clone = Arc::clone(&tool_calls);

        // Process Claude's streaming events and extract text deltas + tool calls
        let chunk_stream = streamed_response.filter_map(move |event: StreamEvent| {
            let tool_calls = Arc::clone(&tool_calls_clone);
            async move {
                match event {
                    StreamEvent::ContentBlockStart { index, content_block } => {
                        // When a tool use block starts, record it
                        if let ContentBlock::ToolUse { id, name, .. } = content_block {
                            let mut calls = tool_calls.lock().unwrap();
                            calls.insert(index, (id, name, String::new()));
                        }
                        None
                    }
                    StreamEvent::ContentBlockDelta { index, delta } => match delta {
                        Delta::TextDelta { text } => {
                            Some(crate::ChatChunk::assistant(crate::ChatPayload::text(text)))
                        }
                        Delta::ThinkingDelta { thinking } => {
                            Some(crate::ChatChunk::assistant(crate::ChatPayload::text(thinking)))
                        }
                        Delta::InputJsonDelta { partial_json } => {
                            // Accumulate the JSON for this tool call
                            let mut calls = tool_calls.lock().unwrap();
                            if let Some((_, _, json)) = calls.get_mut(&index) {
                                json.push_str(&partial_json);
                            }
                            None
                        }
                    },
                    StreamEvent::ContentBlockStop { index } => {
                        // When a tool use block ends, emit the complete tool call
                        let mut calls = tool_calls.lock().unwrap();
                        if let Some((id, name, json)) = calls.remove(&index) {
                            let arguments: serde_json::Value =
                                serde_json::from_str(&json).unwrap_or(serde_json::Value::Null);
                            let tool_call = crate::api::ToolCall {
                                id,
                                name,
                                arguments,
                                extra: serde_json::Value::Null,
                            };
                            return Some(crate::ChatChunk::assistant(crate::ChatPayload::tool_call(
                                tool_call,
                            )));
                        }
                        None
                    }
                    StreamEvent::Error { error } => {
                        warn!(
                            "Received error event: {} - {}",
                            error.error_type, error.message
                        );
                        None
                    }
                    // Ignore other event types (MessageStart, MessageDelta, MessageStop, Ping)
                    _ => None,
                }
            }
        });

        Ok(Box::pin(chunk_stream))
    }
}
