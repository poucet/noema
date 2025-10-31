use std::vec;

use serde::{Deserialize, Serialize};

use crate::{ChatPayload, ChatRequest};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelDefinition {
    pub(crate) name: String,

    pub(crate) version: String,

    pub(crate) display_name: Option<String>,

    pub(crate) description: Option<String>,

    pub(crate) input_token_limit: Option<u32>,

    pub(crate) output_token_limit: Option<u32>,

    pub(crate) thinking: Option<bool>,

    pub(crate) supported_generation_methods: Option<Vec<String>>,
    //TODO:
    // temperature
    // maxTemperature
    // topP
    // topK
}

impl From<ModelDefinition> for crate::ModelDefinition {
    fn from(model: ModelDefinition) -> Self {
        let mut capabilities = Vec::new();

        if let Some(methods) = &model.supported_generation_methods {
            for method in methods {
                match method.as_str() {
                    "generateContent" => {
                        if !capabilities.contains(&crate::ModelCapability::Text) {
                            capabilities.push(crate::ModelCapability::Text);
                        }
                        // Note: Gemini models with generateContent support multimodal input (text + images)
                        // but this is not explicitly indicated in supportedGenerationMethods.
                        // Image capability refers to vision/multimodal input support, which is inherent
                        // to generateContent models in Gemini.
                        if !capabilities.contains(&crate::ModelCapability::Image) {
                            capabilities.push(crate::ModelCapability::Image);
                        }
                    }
                    "embedContent" => {
                        if !capabilities.contains(&crate::ModelCapability::Embedding) {
                            capabilities.push(crate::ModelCapability::Embedding);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Fallback: if no supported methods found, assume text generation
        if capabilities.is_empty() {
            capabilities.push(crate::ModelCapability::Text);
        }

        crate::ModelDefinition::new(model.name, capabilities)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListModelsResponse {
    pub(crate) models: Vec<ModelDefinition>,

    pub(crate) next_page_token: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Model,
}

impl TryFrom<crate::api::Role> for Role {
    type Error = anyhow::Error;

    fn try_from(value: crate::api::Role) -> Result<Self, Self::Error> {
        match value {
            crate::api::Role::User => Ok(Role::User),
            crate::api::Role::Assistant => Ok(Role::Model),
            crate::api::Role::System => Err(anyhow::anyhow!(
                "Gemini does not support system messages directly."
            )),
        }
    }
}

impl From<Role> for crate::api::Role {
    fn from(value: Role) -> Self {
        match value {
            Role::User => crate::api::Role::User,
            Role::Model => crate::api::Role::Assistant,
        }
    }
}

type Blob = serde_json::Value; // Placeholder for actual Blob type

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct GeminiFunctionCall {
    pub(crate) name: String,
    pub(crate) args: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct GeminiFunctionResponse {
    pub(crate) name: String,
    pub(crate) response: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PartType {
    Text(String),
    Image(Blob),
    FunctionCall(GeminiFunctionCall),
    FunctionResponse(GeminiFunctionResponse),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Part {
    pub(crate) thought: Option<bool>,

    #[serde(flatten)]
    pub(crate) data: PartType,

    #[serde(flatten)]
    pub(crate) extra: Option<serde_json::Value>,
}

impl Part {
    pub fn new_text(text: String) -> Self {
        Part {
            thought: None,
            data: PartType::Text(text),
            extra: None,
        }
    }
}

impl From<&Part> for Option<crate::api::ContentBlock> {
    fn from(part: &Part) -> Self {
        match &part.data {
            PartType::Text(t) => Some(crate::api::ContentBlock::Text { text: t.clone() }),
            PartType::Image(_) => Some(crate::api::ContentBlock::Text {
                text: "[Image]".to_string(),
            }),
            PartType::FunctionCall(fc) => Some(crate::api::ContentBlock::ToolCall(
                crate::api::ToolCall {
                    id: format!("gemini_{}", fc.name), // Gemini doesn't provide IDs
                    name: fc.name.clone(),
                    arguments: fc.args.clone(),
                },
            )),
            PartType::FunctionResponse(fr) => Some(crate::api::ContentBlock::ToolResult(
                crate::api::ToolResult {
                    tool_call_id: format!("gemini_{}", fr.name),
                    content: serde_json::to_string(&fr.response).unwrap_or_default(),
                },
            )),
        }
    }
}

impl From<&crate::api::ContentBlock> for Part {
    fn from(block: &crate::api::ContentBlock) -> Self {
        match block {
            crate::api::ContentBlock::Text { text } => Part::new_text(text.clone()),
            crate::api::ContentBlock::ToolCall(call) => Part {
                thought: None,
                data: PartType::FunctionCall(GeminiFunctionCall {
                    name: call.name.clone(),
                    args: call.arguments.clone(),
                }),
                extra: None,
            },
            crate::api::ContentBlock::ToolResult(result) => Part {
                thought: None,
                data: PartType::FunctionResponse(GeminiFunctionResponse {
                    name: result.tool_call_id.clone(),
                    response: serde_json::from_str(&result.content)
                        .unwrap_or(serde_json::Value::String(result.content.clone())),
                }),
                extra: None,
            },
        }
    }
}

// Gemini representation of messages.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Content {
    pub(crate) role: Role,
    pub(crate) parts: Vec<Part>,
}

impl From<&Content> for crate::ChatMessage {
    fn from(content: &Content) -> Self {
        let blocks: Vec<crate::api::ContentBlock> = content
            .parts
            .iter()
            .filter_map(|p| Option::<crate::api::ContentBlock>::from(p))
            .collect();

        crate::ChatMessage::new(content.role.into(), ChatPayload::new(blocks))
    }
}

impl From<Content> for crate::ChatMessage {
    fn from(content: Content) -> Self {
        Self::from(&content)
    }
}

impl From<&Content> for crate::ChatChunk {
    fn from(content: &Content) -> Self {
        let blocks: Vec<crate::api::ContentBlock> = content
            .parts
            .iter()
            .filter_map(|p| Option::<crate::api::ContentBlock>::from(p))
            .collect();

        crate::ChatChunk::new(content.role.into(), ChatPayload::new(blocks))
    }
}

impl From<Content> for crate::ChatChunk {
    fn from(content: Content) -> Self {
        Self::from(&content)
    }
}

trait FromWithRole<T> {
   fn from_with_role(t: T, role: crate::api::Role) -> Self;
}

impl FromWithRole<&crate::ChatPayload> for Content {
    fn from_with_role(payload: &crate::ChatPayload, role: crate::api::Role) -> Self {
        Content {
            role: role.try_into().expect("Invalid role"),
            parts: payload.content.iter().map(|b| b.into()).collect(),
        }
    }
}

impl From<&crate::ChatMessage> for Content {
    fn from(msg: &crate::ChatMessage) -> Self {
        Content::from_with_role(&msg.payload, msg.role)
    }
}

impl From<crate::ChatMessage> for Content {
    // TODO: Don't rely on the const-ref version to avoid cloning, but do this in a way that keeps code-maintenance easy.
    fn from(msg: crate::ChatMessage) -> Self {
        Content::from_with_role(&msg.payload, msg.role)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct GeminiFunctionDeclaration {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    pub(crate) parameters: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiTool {
    pub(crate) function_declarations: Vec<GeminiFunctionDeclaration>,
}

impl From<&Vec<crate::api::ToolDefinition>> for GeminiTool {
    fn from(tools: &Vec<crate::api::ToolDefinition>) -> Self {
        GeminiTool {
            function_declarations: tools
                .iter()
                .map(|t| GeminiFunctionDeclaration {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: serde_json::to_value(&t.input_schema)
                        .expect("Failed to serialize tool schema"),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerateContentRequest {
    pub(crate) contents: Vec<Content>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tools: Option<Vec<GeminiTool>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) system_instruction: Option<Content>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) generation_config: Option<serde_json::Value>,
}

impl GenerateContentRequest {
    pub fn new(contents: Vec<Content>, system_instruction: Option<Content>) -> Self {
        GenerateContentRequest {
            contents,
            tools: None,
            system_instruction,
            generation_config: None,
        }
    }
}

impl From<&ChatRequest> for GenerateContentRequest {
    fn from(request: &ChatRequest) -> Self {
        // Separate system messages because they need to go into the system_messages field.
        let system_instruction = Content {
            parts: request
                .messages
                .iter()
                .filter(|m| m.role == crate::api::Role::System)
                .flat_map(|m| m.payload.content.iter().map(|p| p.into()))
                .collect::<Vec<Part>>(),
            role: Role::User, // Role is ignored for system messages
        };
        let contents = request
            .messages
            .iter()
            .filter(|m| m.role != crate::api::Role::System)
            .map(|msg| msg.into())
            .collect::<Vec<Content>>();

        let tools = request
            .tools
            .as_ref()
            .map(|tools| vec![GeminiTool::from(tools)]);

        let mut req = GenerateContentRequest::new(contents, Some(system_instruction));
        req.tools = tools;
        req
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Candidate {
    pub(crate) content: Content,

    #[serde(flatten)]
    pub(crate) extra: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct GenerateContentResponse {
    pub(crate) candidates: Vec<Candidate>,

    #[serde(flatten)]
    pub(crate) extra: Option<serde_json::Value>,
}

impl From<GenerateContentResponse> for crate::ChatMessage {
    fn from(response: GenerateContentResponse) -> Self {
        // TODO: Move out of candidates instead of cloning.
        response.candidates.first().unwrap().content.clone().into()
    }
}

impl From<GenerateContentResponse> for crate::ChatChunk {
    fn from(response: GenerateContentResponse) -> Self {
        // TODO: Move out of candidates instead of cloning.
        response.candidates.first().unwrap().content.clone().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_content_serialization() {
        let content = Content {
            role: Role::User,
            parts: vec![Part {
                thought: Some(true),
                data: PartType::Text("Hello, world!".to_string()),
                extra: Some(serde_json::json!({"foo": "bar"})),
            }],
        };
        let json = serde_json::to_string(&content).unwrap();
        assert_eq!(
            json,
            r#"{"role":"user","parts":[{"thought":true,"text":"Hello, world!","foo":"bar"}]}"#
        );
    }
}
