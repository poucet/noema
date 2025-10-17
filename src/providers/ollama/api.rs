use serde::{Deserialize, Serialize};
use crate::{ChatRequest, Role};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct ModelDefinition {
    pub (crate) name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct ListModelsResponse {
    pub (crate) models: Vec<ModelDefinition>,
}

// Ollama representation of messages.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct Message {
    pub (crate) role: Role,
    pub (crate) content: String,
}

// TODO: Can we autoderive most of these From classes for values and for references to values?
impl From<Message> for crate::ChatMessage {
    fn from(msg: Message) -> Self {
        crate::ChatMessage {
            role: msg.role,
            content: msg.content,
        }
    }
}

impl From<&crate::ChatMessage> for Message {
    fn from(msg: &crate::ChatMessage) -> Message {
        Message {
            role: msg.role,
            content: msg.content.clone(),
        }
    }
}

impl From<Message> for crate::ChatChunk {
    fn from(msg: Message) -> Self {
        crate::ChatChunk {
            role: msg.role,
            content: msg.content,
        }
    }
}

impl From<crate::ChatChunk> for Message {
    fn from(value: crate::ChatChunk) -> Self {
        Message {
            role: value.role,
            content: value.content,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct OllamaRequest {
    pub (crate) model: String,

    pub (crate) messages: Vec<Message>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub (crate) stream: Option<bool>,
}

impl OllamaRequest {
    pub (crate) fn from_chat_request(model_name: &str, value: &ChatRequest, stream: bool) -> Self {
        let ollama_messages: Vec<_> = value.messages
            .iter()
            .map(|msg| msg.into())
            .collect();
        
        OllamaRequest {
            model: model_name.to_string(),
            messages: ollama_messages,
            stream: Some(stream),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct OllamaResponse {
    pub (crate) message: Message,

    #[serde(flatten)]
    pub (crate) extra: serde_json::Value,
}

impl From<OllamaResponse> for crate::ChatMessage {
    fn from(response: OllamaResponse) -> Self {
        response.message.into()
    }   
}

impl From<OllamaResponse> for crate::ChatChunk {
    fn from(response: OllamaResponse) -> Self {
        response.message.into()
    }   
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
        let request = OllamaRequest {
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
        let request = OllamaRequest {
            model: "test-model".to_string(),
            messages,
            stream: Some(false),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"model":"test-model","messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi there!"}], "stream":false}"#);
    }
}