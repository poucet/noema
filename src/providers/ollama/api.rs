use serde::{Deserialize, Serialize};
use crate::{Role};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct ModelDefinition {
    pub (crate) name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct ListModelsResponse {
    pub (crate) models: Vec<ModelDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct Message {
    pub (crate) role: Role,
    pub (crate) content: String,
}

impl From<Message> for crate::ChatMessage {
    fn from(msg: Message) -> Self {
        crate::ChatMessage {
            role: msg.role,
            content: msg.content,
        }
    }
}

impl Into<Message> for crate::ChatMessage {
    fn into(self) -> Message {
        Message {
            role: self.role,
            content: self.content,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct ChatRequest {
    pub (crate) model: String,

    pub (crate) messages: Vec<Message>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub (crate) stream: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct ChatResponse {
    pub (crate) message: Message,

    #[serde(flatten)]
    pub (crate) extra: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    #[test]
    fn test_ollama_chat_request_serialization() {
        let messages = vec![
            Message {
                role: Role::User,
                content: "Hello".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "Hi there!".to_string(),
            },
        ];
        let request = ChatRequest {
            model: "test-model".to_string(),
            messages,
            stream: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"model":"test-model","messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi there!"}]}"#);
    }

        #[test]
    fn test_ollama_chat_request_serialization_no_stream() {
        let messages = vec![
            Message {
                role: Role::User,
                content: "Hello".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "Hi there!".to_string(),
            },
        ];
        let request = ChatRequest {
            model: "test-model".to_string(),
            messages,
            stream: Some(false),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"model":"test-model","messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi there!"}], "stream":false}"#);
    }
}