//! Chat-related Tauri commands

use llm::{create_model, list_all_models, ChatPayload, ContentBlock};
use noema_core::{ChatEngine, EngineEvent, McpRegistry, SessionStore};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;
use crate::types::{Attachment, ConversationInfo, DisplayMessage, ModelInfo};

/// Get current messages in the conversation
#[tauri::command]
pub async fn get_messages(state: State<'_, AppState>) -> Result<Vec<DisplayMessage>, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let session_arc = engine.get_session();
    let session = session_arc.lock().await;

    Ok(session
        .messages()
        .iter()
        .map(DisplayMessage::from_chat_message)
        .collect())
}

/// Send a message and get streaming responses via events
#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
) -> Result<(), String> {
    let payload = ChatPayload::text(message);
    send_message_internal(app, state, payload).await
}

/// Send a message with attachments
#[tauri::command]
pub async fn send_message_with_attachments(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
    attachments: Vec<Attachment>,
) -> Result<(), String> {
    // Build content blocks from message and attachments
    let mut content = Vec::new();

    // Add text if non-empty
    if !message.trim().is_empty() {
        content.push(ContentBlock::Text { text: message });
    }

    // Add attachments
    for attachment in attachments {
        // Map crate::types::Attachment to noema_ext::Attachment
        let ext_attachment = noema_ext::Attachment {
            name: attachment.name.clone(),
            mime_type: attachment.mime_type.clone(),
            data: attachment.data.clone(),
            size: attachment.size,
        };

        match noema_ext::process_attachment(&ext_attachment) {
            Ok(blocks) => content.extend(blocks),
            Err(e) => return Err(e),
        }
    }

    if content.is_empty() {
        return Err("Message must have text or attachments".to_string());
    }

    let payload = ChatPayload { content };
    send_message_internal(app, state, payload).await
}

/// Internal helper for sending messages
async fn send_message_internal(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: ChatPayload,
) -> Result<(), String> {
    // Emit user message immediately
    let user_msg = DisplayMessage::from_payload(&payload);
    app.emit("user_message", &user_msg)
        .map_err(|e| e.to_string())?;

    // Send to engine - the event loop (started at init) will handle the response
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;
    engine.send_message(payload);

    Ok(())
}

/// Start the engine event polling loop - runs continuously from app init
pub fn start_engine_event_loop(app: AppHandle) {
    tokio::spawn(async move {
        let state = app.state::<AppState>();

        loop {
            let event = {
                let mut engine_guard = state.engine.lock().await;
                match engine_guard.as_mut() {
                    Some(engine) => engine.try_recv(),
                    None => {
                        // Engine not yet initialized, wait and retry
                        drop(engine_guard);
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                        continue;
                    }
                }
            };

            match event {
                Some(EngineEvent::Message(msg)) => {
                    // Mark as processing when we start receiving message chunks
                    *state.is_processing.lock().await = true;
                    let display_msg = DisplayMessage::from_chat_message(&msg);
                    let _ = app.emit("streaming_message", &display_msg);
                }
                Some(EngineEvent::MessageComplete) => {
                    // Get all messages after completion
                    let messages = {
                        let engine_guard = state.engine.lock().await;
                        if let Some(engine) = engine_guard.as_ref() {
                            let session_arc = engine.get_session();
                            let session = session_arc.lock().await;
                            session
                                .messages()
                                .iter()
                                .map(DisplayMessage::from_chat_message)
                                .collect::<Vec<_>>()
                        } else {
                            vec![]
                        }
                    };
                    let _ = app.emit("message_complete", &messages);
                    *state.is_processing.lock().await = false;
                }
                Some(EngineEvent::Error(err)) => {
                    let _ = app.emit("error", &err);
                    *state.is_processing.lock().await = false;
                }
                Some(EngineEvent::ModelChanged(name)) => {
                    let _ = app.emit("model_changed", &name);
                }
                Some(EngineEvent::HistoryCleared) => {
                    let _ = app.emit("history_cleared", ());
                }
                None => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
            }
        }
    });
}

/// Clear conversation history
#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;
    engine.clear_history();
    Ok(())
}

/// Set the current model
#[tauri::command]
pub async fn set_model(
    state: State<'_, AppState>,
    model_id: String,
    provider: String,
) -> Result<String, String> {
    // Construct full model ID as "provider/model"
    let full_model_id = format!("{}/{}", provider, model_id);

    let new_model = create_model(&full_model_id)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    let display_name = model_id
        .split('/')
        .last()
        .unwrap_or(&model_id)
        .to_string();

    {
        let mut engine_guard = state.engine.lock().await;
        let engine = engine_guard.as_mut().ok_or("App not initialized")?;
        engine.set_model(new_model);
    }

    *state.model_id.lock().await = full_model_id;
    *state.model_name.lock().await = display_name.clone();

    Ok(display_name)
}

/// List available models from all providers
#[tauri::command]
pub async fn list_models(_state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let mut all_models = Vec::new();

    for (provider_name, result) in list_all_models().await {
        if let Ok(models) = result {
            for m in models {
                all_models.push(ModelInfo {
                    id: m.definition.id.clone(),
                    display_name: m.definition.name().to_string(),
                    provider: provider_name.clone(),
                });
            }
        }
    }

    Ok(all_models)
}

/// List all conversations
#[tauri::command]
pub async fn list_conversations(state: State<'_, AppState>) -> Result<Vec<ConversationInfo>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    store
        .list_conversations()
        .map(|convos| convos.into_iter().map(ConversationInfo::from).collect())
        .map_err(|e| format!("Failed to list conversations: {}", e))
}

/// Switch to a different conversation
#[tauri::command]
pub async fn switch_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<DisplayMessage>, String> {
    use noema_core::SessionStore;

    let session = {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("App not initialized")?;
        store
            .open_conversation(&conversation_id)
            .map_err(|e| format!("Failed to open conversation: {}", e))?
    };

    let model_id_str = state.model_id.lock().await.clone();
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    // Create model
    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    // Get messages before creating new engine
    let messages: Vec<DisplayMessage> = session
        .messages()
        .iter()
        .map(DisplayMessage::from_chat_message)
        .collect();

    let engine = ChatEngine::new(session, model, mcp_registry);

    *state.engine.lock().await = Some(engine);
    *state.current_conversation_id.lock().await = conversation_id;

    Ok(messages)
}

/// Create a new conversation
#[tauri::command]
pub async fn new_conversation(state: State<'_, AppState>) -> Result<String, String> {
    let session = {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("App not initialized")?;
        store
            .create_conversation()
            .map_err(|e| format!("Failed to create conversation: {}", e))?
    };

    let conversation_id = session.conversation_id().to_string();
    let model_id_str = state.model_id.lock().await.clone();
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    let engine = ChatEngine::new(session, model, mcp_registry);

    *state.engine.lock().await = Some(engine);
    *state.current_conversation_id.lock().await = conversation_id.clone();

    Ok(conversation_id)
}

/// Delete a conversation
#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<(), String> {
    let current_id = state.current_conversation_id.lock().await.clone();
    if conversation_id == current_id {
        return Err("Cannot delete current conversation".to_string());
    }

    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    store
        .delete_conversation(&conversation_id)
        .map_err(|e| format!("Failed to delete conversation: {}", e))
}

/// Rename a conversation
#[tauri::command]
pub async fn rename_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
    name: String,
) -> Result<(), String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    let name_opt = if name.trim().is_empty() {
        None
    } else {
        Some(name.as_str())
    };

    store
        .rename_conversation(&conversation_id, name_opt)
        .map_err(|e| format!("Failed to rename conversation: {}", e))
}

/// Get current model name
#[tauri::command]
pub async fn get_model_name(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.model_name.lock().await.clone())
}

/// Get current conversation ID
#[tauri::command]
pub async fn get_current_conversation_id(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.current_conversation_id.lock().await.clone())
}
