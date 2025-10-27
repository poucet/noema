use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: schemars::schema::RootSchema,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatPayload {
    pub content: Vec<ContentBlock>,
}

impl ChatPayload {
    pub fn new(content: Vec<ContentBlock>) -> Self {
        ChatPayload { content }
    }

    pub fn text(text: impl Into<String>) -> Self {
        ChatPayload {
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    pub fn with_tool_calls(text: String, tool_calls: Vec<ToolCall>) -> Self {
        let mut content = vec![ContentBlock::Text { text }];
        content.extend(tool_calls.into_iter().map(ContentBlock::ToolCall));
        ChatPayload { content }
    }

    pub fn tool_result(tool_call_id: String, result: String) -> Self {
        ChatPayload {
            content: vec![ContentBlock::ToolResult(ToolResult {
                tool_call_id,
                content: result,
            })],
        }
    }

    pub fn get_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn get_tool_calls(&self) -> Vec<&ToolCall> {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolCall(call) => Some(call),
                _ => None,
            })
            .collect()
    }

    pub fn get_tool_results(&self) -> Vec<&ToolResult> {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolResult(result) => Some(result),
                _ => None,
            })
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: Role,
    #[serde(flatten)]
    pub payload: ChatPayload,
}

impl ChatMessage {
    pub fn new(role: Role, payload: ChatPayload) -> Self {
        Self { role, payload }
    }

    pub fn user(payload: ChatPayload) -> Self {
        Self::new(Role::User, payload)
    }
    
    pub fn assistant(payload: ChatPayload) -> Self {
        Self::new(Role::Assistant, payload)
    }

    pub fn system(payload: ChatPayload) -> Self {
        Self::new(Role::System, payload)
    }

    pub fn get_text(&self) -> String {
        self.payload.get_text()
    }

    pub fn get_tool_calls(&self) -> Vec<&ToolCall> {
        self.payload.get_tool_calls()
    }

    pub fn get_tool_results(&self) -> Vec<&ToolResult> {
        self.payload.get_tool_results()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatChunk {
    pub role: Role,
    #[serde(flatten)]
    pub payload: ChatPayload,
}

impl ChatChunk {
    pub fn new(role: Role, payload: ChatPayload) -> Self {
        Self { role, payload }
    }

    pub fn user(payload: ChatPayload) -> Self {
        Self::new(Role::User, payload)
    }
    
    pub fn assistant(payload: ChatPayload) -> Self {
        Self::new(Role::Assistant, payload)
    }

    pub fn system(payload: ChatPayload) -> Self {
        Self::new(Role::System, payload)
    }

    pub fn get_text(&self) -> String {
        self.payload.get_text()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatRequest {
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) tools: Option<Vec<ToolDefinition>>,
}

impl ChatRequest {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        ChatRequest {
            messages,
            tools: None,
        }
    }

    pub fn with_tools(messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Self {
        ChatRequest {
            messages,
            tools: Some(tools),
        }
    }
}
