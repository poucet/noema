use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    #[default]
    Assistant,
    System,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct ChatMessage {
    #[serde(default)]
    pub role: Role,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatChunk {
    pub role: Role,
    pub content: String,
}

impl From<ChatChunk> for ChatMessage {
    fn from(chunk: ChatChunk) -> Self {
        ChatMessage {
            role: chunk.role,
            content: chunk.content,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatRequest {
    pub(crate) messages: Vec<ChatMessage>,
}

impl ChatRequest {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        ChatRequest { messages }
    }
}
