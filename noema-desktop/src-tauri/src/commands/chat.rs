//! Chat-related Tauri commands

use llm::{ChatMessage, ChatPayload, ContentBlock, Role, create_model, list_all_models};
use noema_core::{ChatEngine, EngineEvent, McpRegistry, ToolConfig as CoreToolConfig};
use noema_core::storage::conversation::{ConversationStore, SpanType};
use noema_core::storage::document::resolver::DocumentResolver;
use noema_core::storage::content::{StoredContent, StoredPayload};
use noema_core::storage::session::SessionStore;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::DisplayContent;
use crate::logging::log_message;
use crate::state::AppState;
use crate::types::{AlternateInfo, Attachment, ConversationInfo, DisplayMessage, InputContentBlock, ModelInfo, ReferencedDocument, ToolConfig};


/// Get current messages in the conversation
#[tauri::command]
pub async fn get_messages(state: State<'_, Arc<AppState>>) -> Result<Vec<DisplayMessage>, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let session_arc = engine.get_session();
    let session = session_arc.lock().await;

    Ok(session
        .messages()
        .iter()
        .map(DisplayMessage::from)
        .collect())
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
            InputContentBlock::DocumentRef { id, title } => {
                llm_content.push(ContentBlock::DocumentRef { id, title });
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
                    // Get all messages after completion
                    let messages = {
                        let engine_guard = state.engine.lock().await;
                        if let Some(engine) = engine_guard.as_ref() {
                            let session_arc = engine.get_session();
                            let session = session_arc.lock().await;
                            session
                                .messages()
                                .iter()
                                .map(DisplayMessage::from)
                                .collect::<Vec<_>>()
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
                Some(EngineEvent::ParallelComplete { span_set_id, alternates }) => {
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
                        "spanSetId": span_set_id,
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
    conversation_id: String,
) -> Result<Vec<DisplayMessage>, String> {
    // Get blob store for resolving asset refs
    let blob_store = {
        let blob_guard = state.blob_store.lock().await;
        blob_guard
            .clone()
            .ok_or("Blob store not initialized")?
    };

    let session = {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("App not initialized")?;

        // Create resolver that reads from blob store
        let resolver = {
            let blob = blob_store.clone();
            move |asset_id: String| {
                let blob = blob.clone();
                async move {
                    blob.get(&asset_id)
                        .await
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                }
            }
        };

        store
            .open_conversation(&conversation_id, resolver)
            .await
            .map_err(|e| format!("Failed to open conversation: {}", e))?
    };

    let model_id_str = state.model_id.lock().await.clone();
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    // Create model
    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    // Get messages before creating new engine
    let chat_messages = session.messages();
    eprintln!("switch_conversation: got {} messages from session", chat_messages.len());
    let messages: Vec<DisplayMessage> = chat_messages
        .iter()
        .map(DisplayMessage::from)
        .collect();
    eprintln!("switch_conversation: converted to {} display messages", messages.len());

    // Get document resolver (store implements DocumentResolver directly)
    let document_resolver: Arc<dyn DocumentResolver> = {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("Storage not initialized")?;
        Arc::clone(store) as Arc<dyn DocumentResolver>
    };

    let engine = ChatEngine::new(session, model, mcp_registry, document_resolver);

    *state.engine.lock().await = Some(engine);
    *state.current_conversation_id.lock().await = conversation_id;

    Ok(messages)
}

/// Create a new conversation
#[tauri::command]
pub async fn new_conversation(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let session = {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("App not initialized")?;
        let user_id = state.user_id.lock().await;
        store
            .create_conversation(&user_id)
            .map_err(|e| format!("Failed to create conversation: {}", e))?
    };

    let conversation_id = session.conversation_id().to_string();
    let model_id_str = state.model_id.lock().await.clone();
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    // Get document resolver (store implements DocumentResolver directly)
    let document_resolver: Arc<dyn DocumentResolver> = {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("Storage not initialized")?;
        Arc::clone(store) as Arc<dyn DocumentResolver>
    };

    let engine = ChatEngine::new(session, model, mcp_registry, document_resolver);

    *state.engine.lock().await = Some(engine);
    *state.current_conversation_id.lock().await = conversation_id.clone();

    Ok(conversation_id)
}

/// Delete a conversation
#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, Arc<AppState>>,
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
        .await
        .map_err(|e| format!("Failed to delete conversation: {}", e))
}

/// Rename a conversation
#[tauri::command]
pub async fn rename_conversation(
    state: State<'_, Arc<AppState>>,
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
        .await
        .map_err(|e| format!("Failed to rename conversation: {}", e))
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
#[tauri::command]
pub async fn send_parallel_message(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    message: String,
    model_ids: Vec<String>,
) -> Result<(), String> {
    if model_ids.is_empty() {
        return Err("At least one model must be selected".to_string());
    }

    let message = ChatMessage::user(llm::ChatPayload::text(message));

    // Emit user message immediately
    let user_msg = DisplayMessage::from(&message);
    app.emit("user_message", &user_msg)
        .map_err(|e| e.to_string())?;

    // Send to engine for parallel processing
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;
    engine.send_parallel_message(message, model_ids);

    Ok(())
}

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

/// Get all alternates (spans) for a SpanSet
#[tauri::command]
pub async fn get_span_set_alternates(
    state: State<'_, Arc<AppState>>,
    span_set_id: String,
) -> Result<Vec<SpanInfoResponse>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let alternates = store
        .get_span_set_alternates(&span_set_id)
        .await
        .map_err(|e| format!("Failed to get alternates: {}", e))?;

    Ok(alternates
        .into_iter()
        .map(|a| SpanInfoResponse {
            id: a.id,
            model_id: a.model_id,
            message_count: a.message_count,
            is_selected: a.is_selected,
            created_at: a.created_at,
        })
        .collect())
}

/// Set the selected span for a SpanSet (switch active alternate)
#[tauri::command]
pub async fn set_selected_span(
    state: State<'_, Arc<AppState>>,
    span_set_id: String,
    span_id: String,
) -> Result<(), String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    store
        .set_selected_span(&span_set_id, &span_id)
        .await
        .map_err(|e| format!("Failed to set selected span: {}", e))
}

/// Get messages from a specific span
#[tauri::command]
pub async fn get_span_messages(
    state: State<'_, Arc<AppState>>,
    span_id: String,
) -> Result<Vec<DisplayMessage>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let messages = store
        .get_span_messages(&span_id)
        .await
        .map_err(|e| format!("Failed to get span messages: {}", e))?;

    Ok(messages
        .into_iter()
        .map(|m| {
            let content = m.payload.content.into_iter().map(DisplayContent::from).collect();
            DisplayMessage {
                role: m.role,
                content,
                span_set_id: None,
                span_id: None,
                alternates: None,
            }
        })
        .collect())
}

/// Get all messages for the current conversation with alternates info
/// This is the main entry point for loading a conversation with full span awareness
#[tauri::command]
pub async fn get_messages_with_alternates(state: State<'_, Arc<AppState>>) -> Result<Vec<DisplayMessage>, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let session_arc = engine.get_session();
    let session = session_arc.lock().await;
    let conversation_id = session.conversation_id().to_string();
    drop(session); // Release session lock before accessing store

    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    // Get the main thread for this conversation
    let thread_id = store
        .get_main_thread_id(&conversation_id)
        .await
        .map_err(|e| format!("Failed to get thread: {}", e))?;

    let thread_id = match thread_id {
        Some(id) => id,
        None => return Ok(vec![]), // No thread yet = no messages
    };

    // Get all span sets for this thread
    let span_sets = store
        .get_thread_span_sets(&thread_id)
        .await
        .map_err(|e| format!("Failed to get span sets: {}", e))?;

    let mut result = Vec::new();

    for span_set_info in span_sets {
        // Get full content for this span set
        let span_set = store
            .get_span_set_with_content(&span_set_info.id)
            .await
            .map_err(|e| format!("Failed to get span set content: {}", e))?;

        if let Some(span_set) = span_set {
            // Get the selected span_id (the one we're showing messages from)
            let selected_span_id = span_set
                .alternates
                .iter()
                .find(|a| a.is_selected)
                .map(|a| a.id.clone())
                .or_else(|| span_set.alternates.first().map(|a| a.id.clone()))
                .unwrap_or_default();

            // Convert alternates to AlternateInfo
            let alternates: Vec<AlternateInfo> = span_set
                .alternates
                .iter()
                .map(|a| {
                    let model_display_name = a.model_id.as_ref().map(|id| {
                        id.split('/').last().unwrap_or(id).to_string()
                    });
                    AlternateInfo {
                        span_id: a.id.clone(),
                        model_id: a.model_id.clone(),
                        model_display_name,
                        message_count: a.message_count,
                        is_selected: a.is_selected,
                    }
                })
                .collect();

            // Convert messages to display content
            for msg in span_set.messages {
                let content = msg.payload.content.into_iter().map(DisplayContent::from).collect();

                result.push(DisplayMessage::with_alternates(
                    msg.role,
                    content,
                    span_set_info.id.clone(),
                    selected_span_id.clone(),
                    alternates.clone(),
                ));
            }
        }
    }

    Ok(result)
}

// ========== Thread/Fork Commands ==========

use crate::types::ThreadInfoResponse;

/// List all threads (branches) for a conversation
#[tauri::command]
pub async fn list_conversation_threads(
    state: State<'_, Arc<AppState>>,
    conversation_id: String,
) -> Result<Vec<ThreadInfoResponse>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let threads = store
        .list_conversation_threads(&conversation_id)
        .await
        .map_err(|e| format!("Failed to list threads: {}", e))?;

    Ok(threads.into_iter().map(ThreadInfoResponse::from).collect())
}

/// Fork result containing both conversation and thread IDs
#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ForkResult {
    pub conversation_id: String,
    pub thread_id: String,
}

/// Fork a conversation from a specific span
/// Creates a NEW CONVERSATION that shares history up to the fork point
/// Returns both conversation_id and thread_id so the frontend can switch to it
#[tauri::command]
pub async fn fork_from_span(
    state: State<'_, Arc<AppState>>,
    span_id: String,
    name: Option<String>,
) -> Result<ForkResult, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;
    let user_id = state.user_id.lock().await.clone();

    // Create a new forked conversation (this creates both conversation and thread)
    let (conversation_id, thread_id) = store
        .create_fork_conversation(&user_id, &span_id, name.as_deref())
        .await
        .map_err(|e| format!("Failed to create fork: {}", e))?;

    Ok(ForkResult {
        conversation_id,
        thread_id,
    })
}

/// Switch to a different thread in the current conversation
#[tauri::command]
pub async fn switch_thread(
    state: State<'_, Arc<AppState>>,
    thread_id: String,
) -> Result<Vec<DisplayMessage>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    // Get thread info to verify it exists
    let thread = store
        .get_thread(&thread_id)
        .await
        .map_err(|e| format!("Failed to get thread: {}", e))?
        .ok_or("Thread not found")?;

    // Update current thread in state
    *state.current_thread_id.lock().await = Some(thread_id.clone());

    // Convert to DisplayMessage
    // For forked threads, we need to walk through span_sets properly
    let span_sets = store
        .get_thread_span_sets(&thread_id)
        .await
        .map_err(|e| format!("Failed to get span sets: {}", e))?;

    let mut result = Vec::new();

    // If this is a forked thread, first get ancestry messages
    if thread.parent_span_id.is_some() {
        let ancestry_messages = store
            .get_thread_messages_with_ancestry(&thread_id)
            .await
            .map_err(|e| format!("Failed to get ancestry messages: {}", e))?;

        for msg in ancestry_messages {
            let content = msg.payload.content.into_iter().map(DisplayContent::from).collect();
            result.push(DisplayMessage {
                role: msg.role,
                content,
                span_set_id: None,
                span_id: None,
                alternates: None,
            });
        }
    } else {
        // Main thread - just get span_sets directly
        for span_set_info in span_sets {
            let span_set = store
                .get_span_set_with_content(&span_set_info.id)
                .await
                .map_err(|e| format!("Failed to get span set content: {}", e))?;

            if let Some(span_set) = span_set {
                let selected_span_id = span_set
                    .alternates
                    .iter()
                    .find(|a| a.is_selected)
                    .map(|a| a.id.clone())
                    .or_else(|| span_set.alternates.first().map(|a| a.id.clone()))
                    .unwrap_or_default();

                let alternates: Vec<AlternateInfo> = span_set
                    .alternates
                    .iter()
                    .map(|a| {
                        let model_display_name = a.model_id.as_ref().map(|id| {
                            id.split('/').last().unwrap_or(id).to_string()
                        });
                        AlternateInfo {
                            span_id: a.id.clone(),
                            model_id: a.model_id.clone(),
                            model_display_name,
                            message_count: a.message_count,
                            is_selected: a.is_selected,
                        }
                    })
                    .collect();

                for msg in span_set.messages {
                    let content = msg.payload.content.into_iter().map(DisplayContent::from).collect();

                    result.push(DisplayMessage::with_alternates(
                        msg.role,
                        content,
                        span_set_info.id.clone(),
                        selected_span_id.clone(),
                        alternates.clone(),
                    ));
                }
            }
        }
    }

    Ok(result)
}

/// Rename a thread
#[tauri::command]
pub async fn rename_thread(
    state: State<'_, Arc<AppState>>,
    thread_id: String,
    name: String,
) -> Result<(), String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let name_opt = if name.trim().is_empty() {
        None
    } else {
        Some(name.as_str())
    };

    store
        .rename_thread(&thread_id, name_opt)
        .await
        .map_err(|e| format!("Failed to rename thread: {}", e))
}

/// Delete a thread (cannot delete main thread)
#[tauri::command]
pub async fn delete_thread(
    state: State<'_, Arc<AppState>>,
    thread_id: String,
) -> Result<(), String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    store
        .delete_thread(&thread_id)
        .await
        .map_err(|e| format!("Failed to delete thread: {}", e))?;

    Ok(())
}

/// Get the current thread ID
#[tauri::command]
pub async fn get_current_thread_id(state: State<'_, Arc<AppState>>) -> Result<Option<String>, String> {
    Ok(state.current_thread_id.lock().await.clone())
}

/// Edit a user message by creating a fork with the new content
/// Returns the new thread ID
#[tauri::command]
pub async fn edit_user_message(
    state: State<'_, Arc<AppState>>,
    span_id: String,
    new_content: String,
) -> Result<String, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    // Get the span_set this span belongs to
    let span_set_id = store
        .get_span_parent_span_set(&span_id)
        .await
        .map_err(|e| format!("Failed to get span's span_set: {}", e))?
        .ok_or("Span not found")?;

    // Get the thread this span_set belongs to
    let thread_id = store
        .get_span_set_thread(&span_set_id)
        .await
        .map_err(|e| format!("Failed to get span_set's thread: {}", e))?
        .ok_or("Thread not found")?;

    // Get the thread to find its conversation_id
    let thread = store
        .get_thread(&thread_id)
        .await
        .map_err(|e| format!("Failed to get thread: {}", e))?
        .ok_or("Thread not found")?;

    // Get span_set info to find the previous span_set (we fork from the span before the edited one)
    let span_sets = store
        .get_thread_span_sets(&thread_id)
        .await
        .map_err(|e| format!("Failed to get span sets: {}", e))?;

    // Find the span_set that contains our span and the one before it
    let mut parent_span_id: Option<String> = None;
    for (i, ss) in span_sets.iter().enumerate() {
        if ss.id == span_set_id {
            // If this is the first span_set, we fork from the thread's parent (if any) or start fresh
            if i > 0 {
                // Get the selected span from the previous span_set
                let prev_ss = &span_sets[i - 1];
                if let Some(ref selected) = prev_ss.selected_span_id {
                    parent_span_id = Some(selected.clone());
                }
            }
            break;
        }
    }

    // Create the forked thread
    let new_thread_id = if let Some(ref parent_id) = parent_span_id {
        store
            .create_fork_thread(&thread.conversation_id, parent_id, Some("Edited"))
            .await
            .map_err(|e| format!("Failed to create fork: {}", e))?
    } else {
        // No previous message to fork from - create a fresh thread
        // This shouldn't happen in normal usage (editing the first message)
        store
            .create_fork_thread(&thread.conversation_id, &span_id, Some("Edited"))
            .await
            .map_err(|e| format!("Failed to create fork: {}", e))?
    };


    // Create a span_set for the user message in the new thread
    let span_set_id = store
        .create_span_set(&new_thread_id, SpanType::User)
        .await
        .map_err(|e| format!("Failed to create span_set: {}", e))?;

    // Create a span within the span_set
    let span_id = store
        .create_span(&span_set_id, None)
        .await
        .map_err(|e| format!("Failed to create span: {}", e))?;

    // Create the stored payload with the edited text
    let content = StoredPayload::new(vec![StoredContent::Text {
        text: new_content,
    }]);

    // Add the message to the span
    store
        .add_span_message(&span_id, Role::User, &content)
        .await
        .map_err(|e| format!("Failed to write edited message: {}", e))?;

    // Update current thread in state
    *state.current_thread_id.lock().await = Some(new_thread_id.clone());

    Ok(new_thread_id)
}
