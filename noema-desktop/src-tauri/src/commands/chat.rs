//! Chat-related Tauri commands

use llm::{ChatMessage, ChatPayload, ContentBlock, Role, create_model, list_all_models};
use noema_core::{ChatEngine, EngineEvent, McpRegistry, ToolConfig as CoreToolConfig};
use noema_core::storage::{ConversationStore, TurnStore, Session, SqliteStore, MessageRole};
use noema_core::storage::DocumentResolver;
use noema_core::storage::ids::{ConversationId, TurnId, SpanId};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::logging::log_message;
use crate::state::AppState;
use crate::types::{ConversationInfo, DisplayMessage, InputContentBlock, ModelInfo, ToolConfig};


/// Get current messages in the conversation (committed + pending)
#[tauri::command]
pub async fn get_messages(state: State<'_, Arc<AppState>>) -> Result<Vec<DisplayMessage>, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let session_arc = engine.get_session();
    let session = session_arc.lock().await;

    // Start with committed messages
    let mut msgs: Vec<DisplayMessage> = session
        .messages_for_display()
        .iter()
        .map(DisplayMessage::from)
        .collect();

    // Add pending messages (not yet committed to storage)
    for pending in session.pending_messages() {
        msgs.push(DisplayMessage::from(pending));
    }

    Ok(msgs)
}

/// Send a message with structured content blocks.
/// Content blocks preserve the exact inline position of text, document references, and attachments.
///
/// # Arguments
/// * `content` - The message content blocks (text, document refs, images, audio)
/// * `tool_config` - Optional configuration for which tools to enable. If None, uses default (all tools enabled).
#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    content: Vec<InputContentBlock>,
    tool_config: Option<ToolConfig>,
) -> Result<(), String> {
    if content.is_empty() {
        return Err("Message must have content".to_string());
    }

    // Convert InputContentBlock to ContentBlock, preserving order
    let mut llm_content = Vec::new();
    for block in content {
        match block {
            InputContentBlock::Text { text } => {
                if !text.is_empty() {
                    llm_content.push(ContentBlock::Text { text });
                }
            }
            InputContentBlock::DocumentRef { id } => {
                llm_content.push(ContentBlock::DocumentRef { id });
            }
            InputContentBlock::Image { data, mime_type } => {
                llm_content.push(ContentBlock::Image { data, mime_type });
            }
            InputContentBlock::Audio { data, mime_type } => {
                llm_content.push(ContentBlock::Audio { data, mime_type });
            }
            InputContentBlock::AssetRef { asset_id, mime_type } => {
                // For AssetRef, we need to load the data from blob storage
                // For now, store as-is and resolve later (similar to DocumentRef)
                // TODO: Resolve asset refs before sending to LLM
                llm_content.push(ContentBlock::Image {
                    data: format!("asset://{}", asset_id),
                    mime_type,
                });
            }
        }
    }

    if llm_content.is_empty() {
        return Err("Message must have text, documents, or attachments".to_string());
    }

    let payload = ChatPayload { content: llm_content };

    // Convert ToolConfig from Tauri types to core types
    let core_tool_config = match tool_config {
        Some(tc) => CoreToolConfig {
            enabled: tc.enabled,
            server_ids: tc.server_ids,
            tool_names: tc.tool_names,
        },
        None => CoreToolConfig::all_enabled(), // Default: all tools enabled
    };

    send_message_internal(app, state, payload, core_tool_config).await
}

/// Internal helper for sending messages
async fn send_message_internal(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    payload: ChatPayload,
    tool_config: CoreToolConfig,
) -> Result<(), String> {
    let message = ChatMessage::user(payload);
    // Emit user message immediately
    let user_msg = DisplayMessage::from(&message);
    app.emit("user_message", &user_msg)
        .map_err(|e| e.to_string())?;

    // Send to engine with tool config - the event loop (started at init) will handle the response
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;
    engine.send_message(message, tool_config);

    Ok(())
}

/// Start the engine event polling loop - runs continuously from app init
pub fn start_engine_event_loop(app: AppHandle) {
    tokio::spawn(async move {
        let state = app.state::<Arc<AppState>>();

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
                    let display_msg = DisplayMessage::from(&msg);
                    let _ = app.emit("streaming_message", &display_msg);
                }
                Some(EngineEvent::MessageComplete) => {
                    // Get all messages after completion (committed + pending)
                    let messages = {
                        let engine_guard = state.engine.lock().await;
                        if let Some(engine) = engine_guard.as_ref() {
                            let session_arc = engine.get_session();
                            let session = session_arc.lock().await;

                            // Start with committed messages
                            let mut msgs: Vec<DisplayMessage> = session
                                .messages_for_display()
                                .iter()
                                .map(DisplayMessage::from)
                                .collect();

                            // Add pending messages (not yet committed to storage)
                            for pending in session.pending_messages() {
                                msgs.push(DisplayMessage::from(pending));
                            }

                            msgs
                        } else {
                            vec![]
                        }
                    };
                    let _ = app.emit("message_complete", &messages);
                    *state.is_processing.lock().await = false;
                }
                Some(EngineEvent::Error(err)) => {
                    log_message(&format!("ENGINE ERROR: {}", err));
                    let _ = app.emit("error", &err);
                    *state.is_processing.lock().await = false;
                }
                Some(EngineEvent::ModelChanged(name)) => {
                    let _ = app.emit("model_changed", &name);
                }
                Some(EngineEvent::HistoryCleared) => {
                    let _ = app.emit("history_cleared", ());
                }
                // Parallel execution events
                Some(EngineEvent::ParallelStreamingMessage { model_id, message }) => {
                    *state.is_processing.lock().await = true;
                    let display_msg = DisplayMessage::from(&message);
                    let _ = app.emit("parallel_streaming_message", serde_json::json!({
                        "modelId": model_id,
                        "message": display_msg
                    }));
                }
                Some(EngineEvent::ParallelModelComplete { model_id, messages }) => {
                    let display_messages: Vec<DisplayMessage> = messages
                        .iter()
                        .map(DisplayMessage::from)
                        .collect();
                    let _ = app.emit("parallel_model_complete", serde_json::json!({
                        "modelId": model_id,
                        "messages": display_messages
                    }));
                }
                Some(EngineEvent::ParallelComplete { turn_id, alternates }) => {
                    let alternates_json: Vec<serde_json::Value> = alternates
                        .iter()
                        .map(|a| serde_json::json!({
                            "spanId": a.span_id,
                            "modelId": a.model_id,
                            "modelDisplayName": a.model_display_name,
                            "messageCount": a.message_count,
                            "isSelected": a.is_selected
                        }))
                        .collect();
                    let _ = app.emit("parallel_complete", serde_json::json!({
                        "turnId": turn_id,
                        "alternates": alternates_json
                    }));
                    *state.is_processing.lock().await = false;
                }
                Some(EngineEvent::ParallelModelError { model_id, error }) => {
                    let _ = app.emit("parallel_model_error", serde_json::json!({
                        "modelId": model_id,
                        "error": error
                    }));
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
pub async fn clear_history(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;
    engine.clear_history();
    Ok(())
}

/// Set the current model
#[tauri::command]
pub async fn set_model(
    state: State<'_, Arc<AppState>>,
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

    *state.model_id.lock().await = full_model_id.clone();
    *state.model_name.lock().await = display_name.clone();

    // Save as default model in settings
    let mut settings = config::Settings::load();
    settings.default_model = Some(full_model_id);
    if let Err(e) = settings.save() {
        log_message(&format!("Warning: Failed to save default model setting: {}", e));
    }

    Ok(display_name)
}

/// List available models from all providers
#[tauri::command]
pub async fn list_models(_state: State<'_, Arc<AppState>>) -> Result<Vec<ModelInfo>, String> {
    use llm::ModelCapability;

    let mut all_models = Vec::new();

    for (provider_name, result) in list_all_models().await {
        if let Ok(models) = result {
            for m in models {
                // Only include models that support text/chat (exclude embedding-only models)
                if !m.definition.has_capability(&ModelCapability::Text) {
                    continue;
                }

                let capabilities: Vec<String> = m
                    .definition
                    .capabilities
                    .iter()
                    .map(|c| format!("{:?}", c))
                    .collect();
                all_models.push(ModelInfo {
                    id: m.definition.id.clone(),
                    display_name: m.definition.name().to_string(),
                    provider: provider_name.clone(),
                    capabilities,
                    context_window: m.definition.context_window,
                });
            }
        }
    }

    Ok(all_models)
}

/// List all conversations for the current user
#[tauri::command]
pub async fn list_conversations(state: State<'_, Arc<AppState>>) -> Result<Vec<ConversationInfo>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;
    let user_id = state.user_id.lock().await.clone();

    store
        .list_conversations(&user_id)
        .await
        .map(|convos| convos.into_iter().map(ConversationInfo::from).collect())
        .map_err(|e| format!("Failed to list conversations: {}", e))
}

/// Switch to a different conversation
#[tauri::command]
pub async fn switch_conversation(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Vec<DisplayMessage>, String> {
    let store = {
        let store_guard = state.store.lock().await;
        store_guard
            .as_ref()
            .ok_or("App not initialized")?
            .clone()
    };
    let coordinator = {
        let coord_guard = state.coordinator.lock().await;
        coord_guard
            .as_ref()
            .ok_or("Coordinator not initialized")?
            .clone()
    };

    // Open session for the conversation
    let session = Session::open(coordinator, conversation_id.clone())
        .await
        .map_err(|e| format!("Failed to open conversation: {}", e))?;

    // Get messages before creating new engine
    let messages: Vec<DisplayMessage> = session
        .messages_for_display()
        .iter()
        .map(DisplayMessage::from)
        .collect();

    let model_id_str = state.model_id.lock().await.clone();
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    // Create model
    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    // Store implements DocumentResolver directly
    let document_resolver: Arc<dyn DocumentResolver> = store;

    let engine = ChatEngine::new(session, model, mcp_registry, document_resolver);

    *state.engine.lock().await = Some(engine);
    *state.current_conversation_id.lock().await = conversation_id.into();

    Ok(messages)
}

/// Create a new conversation
#[tauri::command]
pub async fn new_conversation(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let store = {
        let store_guard = state.store.lock().await;
        store_guard
            .as_ref()
            .ok_or("App not initialized")?
            .clone()
    };
    let coordinator = {
        let coord_guard = state.coordinator.lock().await;
        coord_guard
            .as_ref()
            .ok_or("Coordinator not initialized")?
            .clone()
    };
    let user_id = state.user_id.lock().await.clone();

    // Create a new conversation
    let conv_id = store
        .create_conversation(&user_id, None)
        .await
        .map_err(|e| format!("Failed to create conversation: {}", e))?;

    // Open session for the new conversation
    let session = Session::open(coordinator, conv_id.clone())
        .await
        .map_err(|e| format!("Failed to open new conversation: {}", e))?;

    let conversation_id = conv_id.as_str().to_string();
    let model_id_str = state.model_id.lock().await.clone();
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    // Store implements DocumentResolver directly
    let document_resolver: Arc<dyn DocumentResolver> = store;

    let engine = ChatEngine::new(session, model, mcp_registry, document_resolver);

    *state.engine.lock().await = Some(engine);
    *state.current_conversation_id.lock().await = conversation_id.clone();

    Ok(conversation_id)
}

/// Delete a conversation
#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<(), String> {
    let current_id = state.current_conversation_id.lock().await.clone();
    if conversation_id.as_str() == current_id {
        return Err("Cannot delete current conversation".to_string());
    }

    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    store
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
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    let name_opt = if name.trim().is_empty() {
        None
    } else {
        Some(name.as_str())
    };

    store
        .rename_conversation(&conversation_id, name_opt)
        .await
        .map_err(|e| format!("Failed to rename conversation: {}", e))
}

/// Get whether the current conversation is marked as private
#[tauri::command]
pub async fn get_conversation_private(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<bool, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    store
        .is_conversation_private(&conversation_id)
        .await
        .map_err(|e| format!("Failed to get conversation privacy: {}", e))
}

/// Set whether a conversation is marked as private
#[tauri::command]
pub async fn set_conversation_private(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    is_private: bool,
) -> Result<(), String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    store
        .set_conversation_private(&conversation_id, is_private)
        .await
        .map_err(|e| format!("Failed to set conversation privacy: {}", e))
}

/// Get current model name
#[tauri::command]
pub async fn get_model_name(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    Ok(state.model_name.lock().await.clone())
}

/// Get current conversation ID
#[tauri::command]
pub async fn get_current_conversation_id(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    Ok(state.current_conversation_id.lock().await.clone())
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

/// Send a message to multiple models in parallel
/// NOTE: Parallel message support is pending re-implementation for the new Session-based engine
#[tauri::command]
pub async fn send_parallel_message(
    _app: AppHandle,
    _state: State<'_, Arc<AppState>>,
    _message: String,
    _model_ids: Vec<String>,
) -> Result<(), String> {
    // TODO: Re-implement parallel message support for Session-based engine
    Err("Parallel message support is pending re-implementation".to_string())
}

// ============================================================================
// Turn/Span/View Commands (Phase 3 - Pending Implementation)
// ============================================================================
//
// The following commands are pending reimplementation with the new Turn/Span/View model:
// - get_span_set_alternates -> use TurnStore::get_spans(turn_id)
// - set_selected_span -> use TurnStore::select_span(view_id, turn_id, span_id)
// - get_span_messages -> use TurnStore::get_messages_with_content(span_id)
// - get_messages_with_alternates -> use TurnStore::get_view_path(view_id)
// - list_conversation_threads -> use TurnStore::get_views(conversation_id)
// - fork_from_span -> use TurnStore::fork_view(view_id, at_turn_id, name)
// - switch_thread -> use TurnStore::get_view_path(view_id)
// - rename_thread -> (view rename not yet implemented)
// - delete_thread -> (view delete not yet implemented)
// - edit_user_message -> use TurnStore::edit_turn(view_id, turn_id, ...)
//
// For now, the basic get_messages command works through SqliteSession
// which uses get_view_path internally.

use crate::types::ThreadInfoResponse;

/// Information about a span (alternate response) for UI display
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanInfoResponse {
    pub id: String,
    pub model_id: Option<String>,
    pub message_count: usize,
    pub is_selected: bool,
    pub created_at: i64,
}

/// Get all alternates (spans) for a turn
/// In the new model, this returns spans at a specific turn
#[tauri::command]
pub async fn get_turn_alternates(
    state: State<'_, Arc<AppState>>,
    turn_id: TurnId,
) -> Result<Vec<SpanInfoResponse>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let spans = store
        .get_spans(&turn_id)
        .await
        .map_err(|e| format!("Failed to get spans: {}", e))?;

    Ok(spans
        .into_iter()
        .map(|s| SpanInfoResponse {
            id: s.id.as_str().to_string(),
            model_id: s.model_id,
            message_count: s.message_count as usize,
            is_selected: false, // Would need view context to determine
            created_at: s.created_at,
        })
        .collect())
}

/// Get messages from a specific span
#[tauri::command]
pub async fn get_span_messages(
    state: State<'_, Arc<AppState>>,
    span_id: SpanId,
) -> Result<Vec<DisplayMessage>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let messages = store
        .get_messages_with_content(&span_id)
        .await
        .map_err(|e| format!("Failed to get span messages: {}", e))?;

    // TODO: Need to resolve content through coordinator
    // For now, return basic messages without resolved content
    Ok(messages
        .into_iter()
        .map(|m| DisplayMessage {
            role: match m.message.role {
                MessageRole::User => Role::User,
                MessageRole::Assistant => Role::Assistant,
                MessageRole::System => Role::System,
                MessageRole::Tool => Role::Assistant, // Tool results rendered like assistant
            },
            content: vec![], // Content needs resolution via coordinator
            turn_id: None,
            span_id: Some(span_id.clone()),
            alternates: None,
        })
        .collect())
}

/// List all views (branches) for a conversation
#[tauri::command]
pub async fn list_conversation_views(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Vec<ThreadInfoResponse>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let views = store
        .get_views(&conversation_id)
        .await
        .map_err(|e| format!("Failed to list views: {}", e))?;

    Ok(views.into_iter().map(ThreadInfoResponse::from).collect())
}

/// Get the current view ID
#[tauri::command]
pub async fn get_current_view_id(state: State<'_, Arc<AppState>>) -> Result<Option<String>, String> {
    Ok(state.current_thread_id.lock().await.clone())
}
