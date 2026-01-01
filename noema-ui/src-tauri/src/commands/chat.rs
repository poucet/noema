//! Chat-related Tauri commands

use llm::{create_model, list_all_models, ChatPayload, ContentBlock};
use noema_core::{ChatEngine, EngineEvent, McpRegistry, SessionStore};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::logging::log_message;
use crate::state::AppState;
use crate::types::{AlternateInfo, Attachment, ConversationInfo, DisplayMessage, ModelInfo, stored_content_to_display};

/// Referenced document for RAG context
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencedDocument {
    pub id: String,
    pub title: String,
}

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

/// Send a message with document references for RAG
/// The document content is prepended to provide context to the LLM
#[tauri::command]
pub async fn send_message_with_documents(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
    attachments: Vec<Attachment>,
    referenced_documents: Vec<ReferencedDocument>,
) -> Result<(), String> {
    // Build content blocks
    let mut content = Vec::new();

    // Fetch document content for RAG context
    let mut doc_context_parts = Vec::new();
    {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("Storage not initialized")?;

        for doc_ref in &referenced_documents {
            // Get document tabs to extract content
            let tabs = store
                .list_document_tabs(&doc_ref.id)
                .map_err(|e| format!("Failed to get document tabs: {}", e))?;

            // Collect markdown content from all tabs
            let mut doc_content = String::new();
            let tab_count = tabs.len();
            for tab in tabs {
                if let Some(markdown) = tab.content_markdown {
                    if !doc_content.is_empty() {
                        doc_content.push_str("\n\n");
                    }
                    if tab_count > 1 {
                        doc_content.push_str(&format!("## {}\n\n", tab.title));
                    }
                    doc_content.push_str(&markdown);
                }
            }

            if !doc_content.is_empty() {
                doc_context_parts.push(format!(
                    "<document id=\"{}\" title=\"{}\">\n{}\n</document>",
                    doc_ref.id, doc_ref.title, doc_content
                ));
            }
        }
    }

    // Build the message with document context
    if !doc_context_parts.is_empty() {
        let doc_context = format!(
            "<referenced_documents>\n{}\n</referenced_documents>\n\n\
            When referring to information from these documents in your response, \
            use markdown links in the format [relevant text](noema://doc/DOCUMENT_ID) \
            where DOCUMENT_ID is the document's id from the document tags above.\n\n{}",
            doc_context_parts.join("\n\n"),
            message
        );
        content.push(ContentBlock::Text { text: doc_context });
    } else if !message.trim().is_empty() {
        content.push(ContentBlock::Text { text: message });
    }

    // Add attachments
    for attachment in attachments {
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
        return Err("Message must have text, documents, or attachments".to_string());
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
                    let display_msg = DisplayMessage::from_chat_message(&message);
                    let _ = app.emit("parallel_streaming_message", serde_json::json!({
                        "modelId": model_id,
                        "message": display_msg
                    }));
                }
                Some(EngineEvent::ParallelModelComplete { model_id, messages }) => {
                    let display_messages: Vec<DisplayMessage> = messages
                        .iter()
                        .map(DisplayMessage::from_chat_message)
                        .collect();
                    let _ = app.emit("parallel_model_complete", serde_json::json!({
                        "modelId": model_id,
                        "messages": display_messages
                    }));
                }
                Some(EngineEvent::ParallelComplete { alternates }) => {
                    let alternates_json: Vec<serde_json::Value> = alternates
                        .iter()
                        .map(|a| serde_json::json!({
                            "modelId": a.model_id,
                            "modelDisplayName": a.model_display_name,
                            "messageCount": a.message_count,
                            "isSelected": a.is_selected
                        }))
                        .collect();
                    let _ = app.emit("parallel_complete", serde_json::json!({
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

/// List all conversations for the current user
#[tauri::command]
pub async fn list_conversations(state: State<'_, AppState>) -> Result<Vec<ConversationInfo>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;
    let user_id = state.user_id.lock().await.clone();

    store
        .list_conversations(&user_id)
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
        .map(DisplayMessage::from_chat_message)
        .collect();
    eprintln!("switch_conversation: converted to {} display messages", messages.len());

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
    state: State<'_, AppState>,
    message: String,
    model_ids: Vec<String>,
) -> Result<(), String> {
    if model_ids.is_empty() {
        return Err("At least one model must be selected".to_string());
    }

    let payload = llm::ChatPayload::text(message);

    // Emit user message immediately
    let user_msg = DisplayMessage::from_payload(&payload);
    app.emit("user_message", &user_msg)
        .map_err(|e| e.to_string())?;

    // Send to engine for parallel processing
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;
    engine.send_parallel_message(payload, model_ids);

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
    state: State<'_, AppState>,
    span_set_id: String,
) -> Result<Vec<SpanInfoResponse>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let alternates = store
        .get_span_set_alternates(&span_set_id)
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
    state: State<'_, AppState>,
    span_set_id: String,
    span_id: String,
) -> Result<(), String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    store
        .set_selected_span(&span_set_id, &span_id)
        .map_err(|e| format!("Failed to set selected span: {}", e))
}

/// Get messages from a specific span
#[tauri::command]
pub async fn get_span_messages(
    state: State<'_, AppState>,
    span_id: String,
) -> Result<Vec<DisplayMessage>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let messages = store
        .get_span_messages(&span_id)
        .map_err(|e| format!("Failed to get span messages: {}", e))?;

    Ok(messages
        .into_iter()
        .map(|m| {
            let role = match m.role {
                llm::Role::User => "user",
                llm::Role::Assistant => "assistant",
                llm::Role::System => "system",
            };
            let content = m.payload.content.iter().map(stored_content_to_display).collect();
            DisplayMessage {
                role: role.to_string(),
                content,
                span_set_id: None,
                alternates: None,
            }
        })
        .collect())
}

/// Get all messages for the current conversation with alternates info
/// This is the main entry point for loading a conversation with full span awareness
#[tauri::command]
pub async fn get_messages_with_alternates(state: State<'_, AppState>) -> Result<Vec<DisplayMessage>, String> {
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
        .map_err(|e| format!("Failed to get thread: {}", e))?;

    let thread_id = match thread_id {
        Some(id) => id,
        None => return Ok(vec![]), // No thread yet = no messages
    };

    // Get all span sets for this thread
    let span_sets = store
        .get_thread_span_sets(&thread_id)
        .map_err(|e| format!("Failed to get span sets: {}", e))?;

    let mut result = Vec::new();

    for span_set_info in span_sets {
        // Get full content for this span set
        let span_set = store
            .get_span_set_with_content(&span_set_info.id)
            .map_err(|e| format!("Failed to get span set content: {}", e))?;

        if let Some(span_set) = span_set {
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
                let msg_role = match msg.role {
                    llm::Role::User => "user",
                    llm::Role::Assistant => "assistant",
                    llm::Role::System => "system",
                };
                let content = msg.payload.content.iter().map(stored_content_to_display).collect();

                result.push(DisplayMessage::with_alternates(
                    msg_role,
                    content,
                    span_set_info.id.clone(),
                    alternates.clone(),
                ));
            }
        }
    }

    Ok(result)
}
