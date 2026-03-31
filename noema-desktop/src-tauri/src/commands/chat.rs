//! Chat-related Tauri commands

use simply_daemon::api::{
    ConversationApi, ConversationInfo as DaemonConversationInfo,
    DaemonEvent, SessionApi, SessionId, ModelApi, UserMessage,
    ToolFilter as DaemonToolFilter,
};
use simply_daemon::types::{ConversationId, InputContent};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::broadcast;

use crate::logging::log_message;
use crate::state::AppState;
use crate::types::{
    ConversationInfo, DisplayMessage, DisplayInputContent,
    ErrorEvent, MessageCompleteEvent, StreamingMessageEvent, UserMessageEvent,
    ModelInfo, ToolConfig,
};

/// Spawn a task that forwards DaemonEvents from a session broadcast to Tauri UI events.
fn spawn_event_forwarder(
    app: AppHandle,
    state: Arc<AppState>,
    conversation_id: ConversationId,
    mut rx: broadcast::Receiver<DaemonEvent>,
) {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => match event {
                    DaemonEvent::UserMessage(msg) => {
                        let _ = app.emit("user_message", UserMessageEvent {
                            conversation_id: conversation_id.clone(),
                            message: DisplayMessage::from(&msg),
                        });
                    }
                    DaemonEvent::AssistantContent(block) => {
                        state.set_processing(&conversation_id, true).await;
                        let content = vec![crate::types::DisplayContent::from(&block)];
                        let msg = DisplayMessage {
                            role: simply_daemon::types::Role::Assistant,
                            content,
                            turn_id: None,
                            span_id: None,
                            alternates: None,
                        };
                        let _ = app.emit("streaming_message", StreamingMessageEvent {
                            conversation_id: conversation_id.clone(),
                            message: msg,
                        });
                    }
                    DaemonEvent::TurnComplete => {
                        let daemon = match state.get_daemon() {
                            Ok(d) => d,
                            Err(_) => break,
                        };
                        let session_id = SessionId::new(conversation_id.as_str());
                        let messages = daemon
                            .get_messages(&session_id)
                            .await
                            .unwrap_or_default()
                            .iter()
                            .map(DisplayMessage::from)
                            .collect();
                        let _ = app.emit("message_complete", MessageCompleteEvent {
                            conversation_id: conversation_id.clone(),
                            messages,
                        });
                        state.set_processing(&conversation_id, false).await;
                    }
                    DaemonEvent::Error(err) => {
                        log_message(&format!("DAEMON ERROR [{}]: {}", conversation_id.as_str(), err));
                        let _ = app.emit("error", ErrorEvent {
                            conversation_id: conversation_id.clone(),
                            error: err,
                        });
                        state.set_processing(&conversation_id, false).await;
                    }
                    _ => {}
                },
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    log_message(&format!("Event forwarder lagged {} events for {}", n, conversation_id.as_str()));
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Get current messages in the conversation
#[tauri::command]
pub async fn get_messages(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Vec<DisplayMessage>, String> {
    let daemon = state.get_daemon()?;
    let session_id = SessionId::new(conversation_id.as_str());

    Ok(daemon
        .get_messages(&session_id)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))?
        .iter()
        .map(DisplayMessage::from)
        .collect())
}

/// Send a message with structured content blocks.
#[tauri::command]
pub async fn send_message(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    content: Vec<DisplayInputContent>,
    tool_config: Option<ToolConfig>,
) -> Result<(), String> {
    if content.is_empty() {
        return Err("Message must have content".to_string());
    }

    let input_content: Vec<InputContent> = content
        .into_iter()
        .filter(|block| !matches!(block, DisplayInputContent::Text { text } if text.is_empty()))
        .map(InputContent::from)
        .collect();

    if input_content.is_empty() {
        return Err("Message must have text, documents, or attachments".to_string());
    }

    let daemon = state.get_daemon()?;
    let session_id = SessionId::new(conversation_id.as_str());

    daemon
        .send_message(&session_id, UserMessage {
            content: input_content,
            tool_filter: tool_config.map(|tc| DaemonToolFilter {
                server_ids: tc.server_ids,
                tool_names: tc.tool_names,
            }),
        })
        .await
        .map_err(|e| format!("Failed to send message: {}", e))
}

/// Clear conversation history
#[tauri::command]
pub async fn clear_history(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<(), String> {
    let daemon = state.get_daemon()?;
    let session_id = SessionId::new(conversation_id.as_str());
    daemon
        .close_session(&session_id)
        .await
        .map_err(|e| format!("Failed to clear history: {}", e))
}

/// Set the model for a conversation
#[tauri::command]
pub async fn set_model(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    model_id: String,
    provider: String,
) -> Result<String, String> {
    let full_model_id = format!("{}/{}", provider, model_id);
    let daemon = state.get_daemon()?;
    let session_id = SessionId::new(conversation_id.as_str());

    daemon
        .set_model(&session_id, &full_model_id)
        .await
        .map_err(|e| format!("Failed to set model: {}", e))?;

    daemon
        .set_default_model(&full_model_id)
        .await
        .map_err(|e| format!("Failed to set default model: {}", e))?;

    let display_name = model_id
        .split('/')
        .last()
        .unwrap_or(&model_id)
        .to_string();

    let mut settings = config::Settings::load();
    settings.default_model = Some(full_model_id);
    if let Err(e) = settings.save() {
        log_message(&format!("Warning: Failed to save default model setting: {}", e));
    }

    Ok(display_name)
}

/// List available models from all providers
#[tauri::command]
pub async fn list_models(state: State<'_, Arc<AppState>>) -> Result<Vec<ModelInfo>, String> {
    let daemon = state.get_daemon()?;
    let all = daemon.list_models().await.map_err(|e| format!("Failed to list models: {}", e))?;

    Ok(all
        .into_iter()
        .filter(|m| m.definition.has_capability(&simply_daemon::types::ModelCapability::Text))
        .map(|m| ModelInfo {
            id: m.definition.id.clone(),
            display_name: m.definition.name().to_string(),
            provider: m.id.provider.clone(),
            capabilities: m.definition.capabilities.iter().map(|c| format!("{:?}", c)).collect(),
            context_window: m.definition.context_window,
        })
        .collect())
}

/// List all conversations for the current user
#[tauri::command]
pub async fn list_conversations(state: State<'_, Arc<AppState>>) -> Result<Vec<ConversationInfo>, String> {
    let daemon = state.get_daemon()?;

    let convos = daemon
        .list_conversations()
        .await
        .map_err(|e| format!("Failed to list conversations: {}", e))?;

    Ok(convos
        .into_iter()
        .map(|c| ConversationInfo {
            id: c.id,
            name: c.name,
            message_count: c.message_count,
            created_at: c.created_at,
        })
        .collect())
}

/// Load a conversation (creating a daemon session for it)
#[tauri::command]
pub async fn load_conversation(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Vec<DisplayMessage>, String> {
    let daemon = state.get_daemon()?;
    let session_id = SessionId::new(conversation_id.as_str());

    let (_info, rx) = daemon
        .resume_session(&session_id)
        .await
        .map_err(|e| format!("Failed to load conversation: {}", e))?;

    spawn_event_forwarder(app, state.inner().clone(), conversation_id.clone(), rx);

    Ok(daemon
        .get_messages(&session_id)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))?
        .iter()
        .map(DisplayMessage::from)
        .collect())
}

/// Create a new conversation
#[tauri::command]
pub async fn new_conversation(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    name: Option<String>,
) -> Result<String, String> {
    let daemon = state.get_daemon()?;

    let conversation_name = name.unwrap_or_else(|| {
        let now = chrono::Utc::now();
        format!("Chat {}", now.format("%b %d, %H:%M"))
    });

    let conv_id = daemon
        .create_conversation(Some(&conversation_name))
        .await
        .map_err(|e| format!("Failed to create conversation: {}", e))?;

    let session_id = SessionId::new(conv_id.as_str());
    let (_info, rx) = daemon
        .resume_session(&session_id)
        .await
        .map_err(|e| format!("Failed to create session: {}", e))?;

    spawn_event_forwarder(app, state.inner().clone(), conv_id.clone(), rx);

    Ok(conv_id.as_str().to_string())
}

/// Delete a conversation
#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<(), String> {
    let daemon = state.get_daemon()?;
    daemon
        .delete_conversation(&conversation_id)
        .await
        .map_err(|e| format!("Failed to delete conversation: {}", e))
}

/// Rename a conversation
#[tauri::command]
pub async fn rename_conversation(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    name: String,
) -> Result<(), String> {
    let daemon = state.get_daemon()?;
    daemon
        .rename_conversation(&conversation_id, &name)
        .await
        .map_err(|e| format!("Failed to rename conversation: {}", e))
}

/// Get current model name
#[tauri::command]
pub async fn get_model_name(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let daemon = state.get_daemon()?;
    let model_id = daemon.default_model_id().await;
    Ok(model_id.split('/').last().unwrap_or(&model_id).to_string())
}

/// Get favorite models
#[tauri::command]
pub async fn get_favorite_models() -> Result<Vec<String>, String> {
    let settings = config::Settings::load();
    Ok(settings.favorite_models)
}

/// Toggle a model as favorite
#[tauri::command]
pub async fn toggle_favorite_model(model_id: String) -> Result<Vec<String>, String> {
    let mut settings = config::Settings::load();
    settings.toggle_favorite_model(&model_id);
    settings.save().map_err(|e| format!("Failed to save settings: {}", e))?;
    Ok(settings.favorite_models)
}
