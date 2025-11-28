//! Commands and Events for async bridge between UI and backend

use llm::{ChatMessage, ChatPayload, ContentBlock, Role, ToolResultContent};
use noema_core::ConversationInfo;

/// Model info for display in dropdown
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub provider: String,
}

/// Commands from UI to Core (async backend)
pub enum AppCommand {
    SendMessage(ChatPayload),
    ClearHistory,
    SetModel { model_id: String, provider: String },
    ListConversations,
    SwitchConversation(String),
    NewConversation,
    DeleteConversation(String),
    RenameConversation { id: String, name: String },
    ListModels,
}

/// Events from Core to UI
#[derive(Debug, Clone)]
pub enum CoreEvent {
    HistoryLoaded(Vec<DisplayMessage>),
    MessageReceived(DisplayMessage),
    /// User message sent - display immediately
    UserMessageSent(DisplayMessage),
    /// Streaming message with full multimodal content
    StreamingMessage(DisplayMessage),
    MessageComplete,
    Error(String),
    ModelChanged(String),
    HistoryCleared,
    ConversationsList(Vec<ConversationInfo>),
    ConversationSwitched(String),
    ConversationCreated(String),
    ConversationRenamed,
    ModelsList(Vec<ModelInfo>),
}

/// Content block for display - mirrors llm::ContentBlock but owned
#[derive(Debug, Clone)]
pub enum DisplayContent {
    Text(String),
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
    ToolCall { name: String, id: String },
    ToolResult { id: String, content: Vec<DisplayToolResultContent> },
}

#[derive(Debug, Clone)]
pub enum DisplayToolResultContent {
    Text(String),
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
}

/// Message for display with full content blocks
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: MessageRole,
    pub content: Vec<DisplayContent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl From<Role> for MessageRole {
    fn from(role: Role) -> Self {
        match role {
            Role::User => MessageRole::User,
            Role::Assistant => MessageRole::Assistant,
            Role::System => MessageRole::System,
        }
    }
}

impl DisplayMessage {
    pub fn from_chat_message(msg: &ChatMessage) -> Self {
        let content = msg
            .payload
            .content
            .iter()
            .map(|block| match block {
                ContentBlock::Text { text } => DisplayContent::Text(text.clone()),
                ContentBlock::Image { data, mime_type } => DisplayContent::Image {
                    data: data.clone(),
                    mime_type: mime_type.clone(),
                },
                ContentBlock::Audio { data, mime_type } => DisplayContent::Audio {
                    data: data.clone(),
                    mime_type: mime_type.clone(),
                },
                ContentBlock::ToolCall(call) => DisplayContent::ToolCall {
                    name: call.name.clone(),
                    id: call.id.clone(),
                },
                ContentBlock::ToolResult(result) => DisplayContent::ToolResult {
                    id: result.tool_call_id.clone(),
                    content: result
                        .content
                        .iter()
                        .map(|c| match c {
                            ToolResultContent::Text { text } => {
                                DisplayToolResultContent::Text(text.clone())
                            }
                            ToolResultContent::Image { data, mime_type } => {
                                DisplayToolResultContent::Image {
                                    data: data.clone(),
                                    mime_type: mime_type.clone(),
                                }
                            }
                            ToolResultContent::Audio { data, mime_type } => {
                                DisplayToolResultContent::Audio {
                                    data: data.clone(),
                                    mime_type: mime_type.clone(),
                                }
                            }
                        })
                        .collect(),
                },
            })
            .collect();

        Self {
            role: msg.role.into(),
            content,
        }
    }

    pub fn from_payload(payload: &ChatPayload) -> Self {
        let content = payload
            .content
            .iter()
            .map(|block| match block {
                ContentBlock::Text { text } => DisplayContent::Text(text.clone()),
                ContentBlock::Image { data, mime_type } => DisplayContent::Image {
                    data: data.clone(),
                    mime_type: mime_type.clone(),
                },
                ContentBlock::Audio { data, mime_type } => DisplayContent::Audio {
                    data: data.clone(),
                    mime_type: mime_type.clone(),
                },
                ContentBlock::ToolCall(call) => DisplayContent::ToolCall {
                    name: call.name.clone(),
                    id: call.id.clone(),
                },
                ContentBlock::ToolResult(result) => DisplayContent::ToolResult {
                    id: result.tool_call_id.clone(),
                    content: result
                        .content
                        .iter()
                        .map(|c| match c {
                            ToolResultContent::Text { text } => {
                                DisplayToolResultContent::Text(text.clone())
                            }
                            ToolResultContent::Image { data, mime_type } => {
                                DisplayToolResultContent::Image {
                                    data: data.clone(),
                                    mime_type: mime_type.clone(),
                                }
                            }
                            ToolResultContent::Audio { data, mime_type } => {
                                DisplayToolResultContent::Audio {
                                    data: data.clone(),
                                    mime_type: mime_type.clone(),
                                }
                            }
                        })
                        .collect(),
                },
            })
            .collect();

        Self {
            role: MessageRole::User,
            content,
        }
    }
}
