use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

impl TryFrom<crate::api::Role> for Role {
    type Error = anyhow::Error;

    fn try_from(value: crate::api::Role) -> Result<Self, Self::Error> {
        match value {
            crate::api::Role::User => Ok(Role::User),
            crate::api::Role::Assistant => Ok(Role::Assistant),
            crate::api::Role::System => Err(anyhow::anyhow!(
                "Claude does not support system messages directly in role field."
            )),
        }
    }
}

impl From<Role> for crate::api::Role {
    fn from(value: Role) -> Self {
        match value {
            Role::User => crate::api::Role::User,
            Role::Assistant => crate::api::Role::Assistant,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub(crate) enum Citation {
    // TODO
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum Content {
    Text {
        text: String,

        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<Citation>>,
    },
}

impl TryFrom<&Content> for crate::ChatMessage {
    type Error = anyhow::Error;

    fn try_from(content: &Content) -> Result<Self, Self::Error> {
        match content {
            Content::Text { citations, text } => Ok(crate::ChatMessage {
                role: crate::api::Role::Assistant,
                content: text.clone(),
            }),
        }
    }
}

impl TryFrom<&Content> for crate::ChatChunk {
    type Error = anyhow::Error;

    fn try_from(content: &Content) -> Result<Self, Self::Error> {
        match content {
            Content::Text { citations, text } => Ok(crate::ChatChunk {
                role: crate::api::Role::Assistant,
                content: text.clone(),
            }),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct InputMessage {
    pub(crate) content: Vec<Content>,

    pub(crate) role: Role,
}

impl From<&crate::ChatMessage> for InputMessage {
    fn from(msg: &crate::ChatMessage) -> InputMessage {
        InputMessage {
            role: msg.role.try_into().expect("Role not understood"),
            content: vec![Content::Text {
                citations: None,
                text: msg.content.clone(),
            }],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum SystemPrompt {
    Text { text: String },
}

impl SystemPrompt {
    fn new(text: &str) -> Self {
        SystemPrompt::Text {
            text: text.to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MessagesRequest {
    pub(crate) model: String,

    pub(crate) messages: Vec<InputMessage>,

    pub(crate) max_tokens: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stream: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) system: Option<SystemPrompt>,
}

impl MessagesRequest {
    pub(crate) fn from_chat_request(
        model_name: &str,
        request: &crate::ChatRequest,
        stream: bool,
    ) -> Self {
        // Separate system messages because they need to go into the system_messages field.
        let system_instruction = request
            .messages
            .iter()
            .filter(|m| m.role == crate::api::Role::System)
            .map(|m| m.content.clone())
            .collect::<Vec<String>>()
            .join("\n");

        let messages = request
            .messages
            .iter()
            .filter(|m| m.role != crate::api::Role::System)
            .map(|msg: &crate::ChatMessage| msg.into())
            .collect::<Vec<InputMessage>>();

        MessagesRequest {
            model: model_name.to_string(),
            messages: messages,
            // TODO: Don't hardcode
            max_tokens: 32000,
            stream: Some(stream),
            system: if system_instruction.len() == 0 {
                None
            } else {
                Some(SystemPrompt::new(&system_instruction))
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MessagesResponse {
    pub(crate) id: String,

    pub(crate) role: Role,

    pub(crate) content: Vec<Content>,

    pub(crate) model: String,

    // Turn into an enum later.
    pub(crate) stop_reason: Option<String>,

    pub(crate) stop_sequence: Option<String>,

    #[serde(flatten)]
    pub(crate) extra: serde_json::Value,
}

impl From<MessagesResponse> for crate::ChatMessage {
    fn from(response: MessagesResponse) -> Self {
        response
            .content
            .first()
            .expect("No content")
            .try_into()
            .expect("Failed to parse Claude response")
    }
}

impl From<MessagesResponse> for crate::ChatChunk {
    fn from(response: MessagesResponse) -> Self {
        response
            .content
            .first()
            .expect("No content")
            .try_into()
            .expect("Failed to parse Claude response")
    }
}
