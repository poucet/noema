use crate::api::{ChatMessage, ChatRequest, Role};
use serde::{Deserialize, Serialize};

/// OpenAI content part for multimodal messages
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    ImageUrl {
        image_url: ImageUrlContent,
    },
    InputAudio {
        input_audio: InputAudioContent,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ImageUrlContent {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InputAudioContent {
    pub data: String,
    pub format: String,
}

/// OpenAI message content - can be a string or array of parts
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub function: FunctionCall,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<MessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl From<&ChatMessage> for Message {
    fn from(msg: &ChatMessage) -> Self {
        let tool_calls: Vec<_> = msg
            .get_tool_calls()
            .iter()
            .map(|tc| ToolCall {
                id: tc.id.clone(),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: tc.name.clone(),
                    arguments: serde_json::to_string(&tc.arguments)
                        .unwrap_or_default(),
                },
            })
            .collect();

        // Check if this is a tool result message
        let is_tool_result = msg.payload.content.iter().any(|b| matches!(b, crate::api::ContentBlock::ToolResult(_)));

        if is_tool_result {
            // For tool results, we need to extract the tool_call_id
            // OpenAI only supports text in tool results, so we concatenate text
            // and add notes about any images/audio
            if let Some(crate::api::ContentBlock::ToolResult(result)) = msg.payload.content.first() {
                let mut text_parts: Vec<String> = Vec::new();
                let mut has_image = false;
                let mut has_audio = false;

                for content in &result.content {
                    match content {
                        crate::api::ToolResultContent::Text { text } => {
                            text_parts.push(text.clone());
                        }
                        crate::api::ToolResultContent::Image { .. } => {
                            has_image = true;
                        }
                        crate::api::ToolResultContent::Audio { .. } => {
                            has_audio = true;
                        }
                    }
                }

                let mut result_text = text_parts.join("");
                if has_image {
                    result_text.push_str("\n[Image data included in result]");
                }
                if has_audio {
                    result_text.push_str("\n[Audio data included in result]");
                }

                return Message {
                    role: msg.role,
                    content: Some(MessageContent::Text(result_text)),
                    tool_calls: None,
                    tool_call_id: Some(result.tool_call_id.clone()),
                };
            }
        }

        // Convert content blocks to OpenAI parts
        let parts: Vec<ContentPart> = msg
            .payload
            .content
            .iter()
            .filter_map(|block| match block {
                crate::api::ContentBlock::Text { text } => {
                    Some(ContentPart::Text { text: text.clone() })
                }
                crate::api::ContentBlock::Image { data, mime_type } => {
                    // OpenAI expects data URLs for base64 images
                    let url = format!("data:{};base64,{}", mime_type, data);
                    Some(ContentPart::ImageUrl {
                        image_url: ImageUrlContent {
                            url,
                            detail: None,
                        },
                    })
                }
                crate::api::ContentBlock::Audio { data, mime_type } => {
                    // Extract format from mime type (e.g., "audio/wav" -> "wav")
                    let format = mime_type
                        .strip_prefix("audio/")
                        .unwrap_or("wav")
                        .to_string();
                    Some(ContentPart::InputAudio {
                        input_audio: InputAudioContent {
                            data: data.clone(),
                            format,
                        },
                    })
                }
                crate::api::ContentBlock::ToolCall(_) | crate::api::ContentBlock::ToolResult(_) => {
                    None // Handled separately
                }
            })
            .collect();

        let content = if parts.is_empty() {
            None
        } else if parts.len() == 1 {
            // Single text part can use simple string format
            if let ContentPart::Text { text } = &parts[0] {
                Some(MessageContent::Text(text.clone()))
            } else {
                Some(MessageContent::Parts(parts))
            }
        } else {
            Some(MessageContent::Parts(parts))
        };

        Message {
            role: msg.role,
            content,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_call_id: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub r#type: String,
    pub function: FunctionDefinition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FunctionDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

impl From<&crate::api::ToolDefinition> for Tool {
    fn from(def: &crate::api::ToolDefinition) -> Self {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: def.name.clone(),
                description: def.description.clone(),
                parameters: serde_json::to_value(&def.input_schema)
                    .expect("Failed to serialize tool schema"),
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

impl ChatCompletionRequest {
    pub fn from_request(model: String, request: &ChatRequest, stream: bool) -> Self {
        let tools = request
            .tools
            .as_ref()
            .map(|tools| tools.iter().map(|t| t.into()).collect());

        ChatCompletionRequest {
            model,
            messages: request.messages.iter().map(|m| m.into()).collect(),
            stream: if stream { Some(true) } else { None },
            tools,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatCompletionChoice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
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
        let mut content = Vec::new();

        // Add content blocks if present
        if let Some(msg_content) = &choice.message.content {
            match msg_content {
                MessageContent::Text(text) => {
                    if !text.is_empty() {
                        content.push(crate::api::ContentBlock::Text { text: text.clone() });
                    }
                }
                MessageContent::Parts(parts) => {
                    for part in parts {
                        match part {
                            ContentPart::Text { text } => {
                                content.push(crate::api::ContentBlock::Text { text: text.clone() });
                            }
                            ContentPart::ImageUrl { image_url } => {
                                // Try to parse data URL format
                                if let Some(data_url) = image_url.url.strip_prefix("data:") {
                                    if let Some((mime_type, data)) = data_url.split_once(";base64,") {
                                        content.push(crate::api::ContentBlock::Image {
                                            data: data.to_string(),
                                            mime_type: mime_type.to_string(),
                                        });
                                    }
                                }
                                // For regular URLs, we'd need to fetch them - skip for now
                            }
                            ContentPart::InputAudio { input_audio } => {
                                let mime_type = format!("audio/{}", input_audio.format);
                                content.push(crate::api::ContentBlock::Audio {
                                    data: input_audio.data.clone(),
                                    mime_type,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Add tool calls if present
        if let Some(tool_calls) = &choice.message.tool_calls {
            for tc in tool_calls {
                let arguments = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(serde_json::Value::Null);

                content.push(crate::api::ContentBlock::ToolCall(crate::api::ToolCall {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    arguments,
                }));
            }
        }

        ChatMessage::assistant(crate::ChatPayload::new(content))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatCompletionChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
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
pub struct ModelCapabilities {
    pub fine_tune: Option<bool>,
    pub inference: Option<bool>,
    pub completion: Option<bool>,
    pub chat_completion: Option<bool>,
    pub embeddings: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Model {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
    // Azure OpenAI provides a capabilities field
    pub capabilities: Option<ModelCapabilities>,
}

impl From<Model> for crate::ModelDefinition {
    fn from(model: Model) -> Self {
        let mut capabilities = Vec::new();

        // Check if Azure OpenAI capabilities field is present
        if let Some(caps) = &model.capabilities {
            if caps.chat_completion == Some(true) || caps.completion == Some(true) {
                capabilities.push(crate::ModelCapability::Text);
            }
            if caps.embeddings == Some(true) {
                capabilities.push(crate::ModelCapability::Embedding);
            }
            // Note: Azure capabilities doesn't have explicit vision field
            // Vision support is not indicated in the capabilities object
        } else {
            // Standard OpenAI API doesn't provide capability fields
            // This is a limitation of the OpenAI API - there's no explicit capability metadata
            // All we can do is assume text capability as default
            capabilities.push(crate::ModelCapability::Text);
        }

        // Fallback: if no capabilities determined, assume text
        if capabilities.is_empty() {
            capabilities.push(crate::ModelCapability::Text);
        }

        crate::ModelDefinition::new(model.id, capabilities)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ListModelsResponse {
    pub object: String,
    pub data: Vec<Model>,
}
