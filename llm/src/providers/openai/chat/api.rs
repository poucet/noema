use crate::api::{ChatMessage, ChatRequest, Role};
use serde::{Deserialize, Serialize};

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
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl From<&ChatMessage> for Message {
    fn from(msg: &ChatMessage) -> Self {
        let text = msg.get_text();
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
            if let Some(crate::api::ContentBlock::ToolResult(result)) = msg.payload.content.first() {
                return Message {
                    role: msg.role,
                    content: Some(result.content.clone()),
                    tool_calls: None,
                    tool_call_id: Some(result.tool_call_id.clone()),
                };
            }
        }

        Message {
            role: msg.role,
            content: if text.is_empty() { None } else { Some(text) },
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

        // Add text content if present
        if let Some(text) = &choice.message.content {
            if !text.is_empty() {
                content.push(crate::api::ContentBlock::Text { text: text.clone() });
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
pub struct Model {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
}

impl From<Model> for crate::ModelDefinition {
    fn from(model: Model) -> Self {
        // All OpenAI models support text/chat
        crate::ModelDefinition::text_model(model.id)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ListModelsResponse {
    pub object: String,
    pub data: Vec<Model>,
}
