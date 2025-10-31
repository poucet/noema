use crate::{ChatRequest, api::Role};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ModelDetails {
    pub(crate) format: Option<String>,
    pub(crate) family: Option<String>,
    pub(crate) families: Option<Vec<String>>,
    pub(crate) parameter_size: Option<String>,
    pub(crate) quantization_level: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ModelDefinition {
    pub(crate) name: String,
    pub(crate) details: Option<ModelDetails>,
}

impl From<ModelDefinition> for crate::ModelDefinition {
    fn from(model: ModelDefinition) -> Self {
        let mut capabilities = Vec::new();
        let mut is_embedding = false;
        let mut has_vision = false;

        // Check details.families for model capabilities
        if let Some(details) = &model.details {
            if let Some(families) = &details.families {
                // Check for vision capability: models with CLIP in families support vision
                if families.iter().any(|f| f.eq_ignore_ascii_case("clip")) {
                    has_vision = true;
                }

                // Check for embedding models: typically don't have standard text generation families
                // Embedding models often have specific family markers
                if families.len() == 1
                    && families
                        .iter()
                        .any(|f| f.to_lowercase().contains("embed"))
                {
                    is_embedding = true;
                }
            }
        }

        // Determine capabilities based on analysis
        if is_embedding {
            capabilities.push(crate::ModelCapability::Embedding);
        } else {
            // Default to text capability for non-embedding models
            capabilities.push(crate::ModelCapability::Text);
            if has_vision {
                capabilities.push(crate::ModelCapability::Image);
            }
        }

        // Fallback: if we couldn't determine anything, assume text
        if capabilities.is_empty() {
            capabilities.push(crate::ModelCapability::Text);
        }

        crate::ModelDefinition::new(model.name, capabilities)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ListModelsResponse {
    pub(crate) models: Vec<ModelDefinition>,
}

// Ollama representation of messages.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Message {
    pub(crate) role: Role,
    pub(crate) content: String,
}

// TODO: Can we autoderive most of these From classes for values and for references to values?
impl From<Message> for crate::ChatMessage {
    fn from(msg: Message) -> Self {
        let role = msg.role;
        let payload = crate::ChatPayload::text(msg.content);
        crate::ChatMessage::new(role, payload)
    }
}

impl From<&crate::ChatMessage> for Message {
    fn from(msg: &crate::ChatMessage) -> Message {
        Message {
            role: msg.role,
            content: msg.get_text(),
        }
    }
}

impl From<Message> for crate::ChatChunk {
    fn from(msg: Message) -> Self {
        let role = msg.role;
        let payload = crate::ChatPayload::text(msg.content);
        crate::ChatChunk::new(role, payload)
    }
}

impl From<crate::ChatChunk> for Message {
    fn from(value: crate::ChatChunk) -> Self {
        Message {
            role: value.role,
            content: value.payload.get_text(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct OllamaTool {
    #[serde(rename = "type")]
    pub(crate) r#type: String,
    pub(crate) function: OllamaFunction,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct OllamaFunction {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    pub(crate) parameters: serde_json::Value,
}

impl From<&crate::api::ToolDefinition> for OllamaTool {
    fn from(def: &crate::api::ToolDefinition) -> Self {
        OllamaTool {
            r#type: "function".to_string(),
            function: OllamaFunction {
                name: def.name.clone(),
                description: def.description.clone(),
                parameters: serde_json::to_value(&def.input_schema)
                    .expect("Failed to serialize tool schema"),
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct OllamaRequest {
    pub(crate) model: String,

    pub(crate) messages: Vec<Message>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stream: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tools: Option<Vec<OllamaTool>>,
}

impl OllamaRequest {
    pub(crate) fn from_chat_request(model_name: &str, value: &ChatRequest, stream: bool) -> Self {
        let ollama_messages: Vec<_> = value.messages.iter().map(|msg| msg.into()).collect();

        let tools = value
            .tools
            .as_ref()
            .map(|tools| tools.iter().map(|t| t.into()).collect());

        OllamaRequest {
            model: model_name.to_string(),
            messages: ollama_messages,
            stream: Some(stream),
            tools,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct OllamaResponse {
    pub(crate) message: Message,

    #[serde(flatten)]
    pub(crate) extra: serde_json::Value,
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
            tools: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"model":"test-model","messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi there!"}]}"#
        );
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
            tools: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"model":"test-model","messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi there!"}],"stream":false}"#
        );
    }
}
