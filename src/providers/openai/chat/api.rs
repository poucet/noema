use crate::api::{ChatMessage, ChatRequest, Role};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl From<&ChatMessage> for Message {
    fn from(msg: &ChatMessage) -> Self {
        Message {
            role: msg.role,
            content: msg.content.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

impl ChatCompletionRequest {
    pub fn from_request(model: String, request: &ChatRequest, stream: bool) -> Self {
        ChatCompletionRequest {
            model,
            messages: request.messages.iter().map(|m| m.into()).collect(),
            stream: if stream { Some(true) } else { None },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatCompletionChoice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatCompletionChoice>,
}

impl From<ChatCompletionResponse> for ChatMessage {
    fn from(response: ChatCompletionResponse) -> Self {
        let choice = &response.choices[0];
        ChatMessage {
            role: choice.message.role,
            content: choice.message.content.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatCompletionChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatCompletionChunkChoice {
    pub index: u32,
    pub delta: ChatCompletionChunkDelta,
    pub finish_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatCompletionChunkChoice>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Model {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ListModelsResponse {
    pub object: String,
    pub data: Vec<Model>,
}
