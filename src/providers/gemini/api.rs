use std::vec;

use serde::{Deserialize, Serialize};

use crate::ChatRequest;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct ModelDefinition {
    pub (crate) name: String,

    pub (crate) version: String,

    #[serde(rename = "displayName")]
    pub (crate) display_name: Option<String>,

    pub (crate) description: Option<String>,

    #[serde(rename = "inputTokenLimit")]
    pub (crate) input_token_limit: Option<u32>, 
    
    #[serde(rename = "outputTokenLimit")]
    pub (crate) output_token_limit: Option<u32>, 

    pub (crate) thinking: Option<bool>, 

    //TODO:
    // temperature
    // maxTemperature
    // topP
    // topK
    // supportedGenerationMethods
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct ListModelsResponse {
    pub (crate) models: Vec<ModelDefinition>,

    #[serde(rename = "nextPageToken")]
    pub (crate) next_page_token: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Model,
}

impl TryFrom<crate::Role> for Role {
    type Error = anyhow::Error;

    fn try_from(value: crate::Role) -> Result<Self, Self::Error> {
        match value {
            crate::Role::User => Ok(Role::User),
            crate::Role::Assistant => Ok(Role::Model),
            crate::Role::System => Err(anyhow::anyhow!("Gemini does not support system messages directly.")),
        }
    }
}

impl From<Role> for crate::Role {
    fn from(value: Role) -> Self {
        match value {
            Role::User => crate::Role::User,
            Role::Model => crate::Role::Assistant,
        }
    }
}

type Blob = serde_json::Value; // Placeholder for actual Blob type
type FunctionCall = serde_json::Value; // Placeholder for actual FunctionCall type
type FunctionResponse = serde_json::Value; // Placeholder for actual FunctionResponse type


#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) enum PartType {
    #[serde(rename = "text")]
    Text(String),
    #[serde(rename = "image")]
    Image(Blob),
    #[serde(rename = "functionCall")]
    FunctionCall(FunctionCall),
    #[serde(rename = "functionResponse")]
    FunctionResponse(FunctionResponse),
}


#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct Part {
    pub (crate) thought: Option<bool>,

    #[serde(flatten)]
    pub (crate) data: PartType,

    #[serde(flatten)]
    pub (crate) extra: Option<serde_json::Value>,
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

impl From<&Part> for crate::ChatMessage {
    fn from(part: &Part) -> Self {
        match &part.data {
            PartType::Text(t) => crate::ChatMessage {
                role: crate::Role::Assistant,
                content: t.clone(),
            },
            PartType::Image(_) => crate::ChatMessage {
                role: crate::Role::Assistant,
                content: "[Image]".to_string(),
            },
            PartType::FunctionCall(_) => crate::ChatMessage {
                role: crate::Role::Assistant,
                content: "[Function Call]".to_string(),
            },
            PartType::FunctionResponse(_) => crate::ChatMessage {
                role: crate::Role::Assistant,
                content: "[Function Response]".to_string(),
            },
        }
    }
}

impl From<&crate::ChatMessage> for Part {
    fn from(msg: &crate::ChatMessage) -> Self {
        Part::new_text(msg.content.clone())
    }
}

// Gemini representation of messages.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct Content {
    pub (crate) role: Role,
    pub (crate) parts: Vec<Part>,
}

impl From<&Content> for crate::ChatMessage {
    // TODO: Handle multiple parts better, currently just takes the first text part.
    fn from(content: &Content) -> Self {
        crate::ChatMessage {
            role: content.role.into(),
            content: content.parts.iter().filter_map(|p| match &p.data {
                PartType::Text(t) => Some(t.clone()),
                PartType::Image(_) => None,
                PartType::FunctionCall(_) => None,
                PartType::FunctionResponse(_) => None,
            }).next().unwrap_or("".to_string()).clone(), // Take first text part or empty
        }
    }
}

impl From<Content> for crate::ChatMessage {
    fn from(content: Content) -> Self {
        Self::from(&content)
    }
}

impl From<Content> for crate::ChatChunk {
    fn from(content: Content) -> Self {
        crate::ChatChunk {
            role: content.role.into(),
            content: content.parts.iter().filter_map(|p| match &p.data {
                PartType::Text(t) => Some(t.clone()),
                PartType::Image(_) => None,
                PartType::FunctionCall(_) => None,
                PartType::FunctionResponse(_) => None,
            }).next().unwrap_or("".to_string()), // Take first text part or empty
        }
    }
}

impl From<&crate::ChatMessage> for Content {
    fn from(msg: &crate::ChatMessage) -> Self {
        Content {
            role: msg.role.try_into().unwrap(), // Safe unwrap because of prior filtering
            parts: vec![Part::new_text(msg.content.clone())],
        }
    }
}

impl From<crate::ChatMessage> for Content {
    // TODO: Don't rely on the const-ref version to avoid cloning, but do this in a way that keeps code-maintenance easy.
    fn from(msg: crate::ChatMessage) -> Self {
        Self::from(&msg)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct GenerateContentRequest {
    pub (crate) contents: Vec<Content>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub (crate) tools: Vec<serde_json::Value>,

    #[serde(rename = "systemInstruction")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub (crate) system_instruction: Option<Content>,

    #[serde(rename = "generationConfig")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub (crate) generation_config: Option<serde_json::Value>,
}

impl GenerateContentRequest {
    pub fn new(contents: Vec<Content>, system_instruction: Option<Content>) -> Self {
        GenerateContentRequest {
            contents,
            tools: vec![],
            system_instruction,
            generation_config: None,
        }
    }
}

impl From<&ChatRequest> for GenerateContentRequest {
    fn from(request: &ChatRequest) -> Self {
        // Separate system messages because they need to go into the system_messages field.
        let system_instruction = Content {
            parts: request.messages.iter().filter(|m| m.role == crate::Role::System)
            .map(|m| m.into()).collect::<Vec<Part>>(),
            role: Role::User, // Role is ignored for system messages   
        };
        let contents = request.messages.iter().filter(|m| m.role != crate::Role::System)
            .map(|msg| msg.into())
            .collect::<Vec<Content>>();

        GenerateContentRequest::new(contents, Some(system_instruction))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct Candidate {
    pub (crate) content: Content,

    #[serde(flatten)]
    pub (crate) extra: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub (crate) struct GenerateContentResponse {
    pub (crate) candidates: Vec<Candidate>,

    #[serde(flatten)]
    pub (crate) extra: Option<serde_json::Value>,
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
            parts: vec![
                Part {
                    thought: Some(true),
                    data: PartType::Text("Hello, world!".to_string()),
                    extra: Some(serde_json::json!({"foo": "bar"})),
                },
            ],
        };
        let json = serde_json::to_string(&content).unwrap();
        assert_eq!(json, r#"{"role":"user","parts":[{"thought":true,"text":"Hello, world!","foo":"bar"}]}"#);
    }
}
