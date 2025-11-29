//! Chat-related Tauri commands

use config::{create_provider, get_model_info, ModelProviderType, ProviderUrls};
use llm::{ChatPayload, ContentBlock, ModelProvider};
use noema_core::{ChatEngine, EngineEvent, McpRegistry, SessionStore};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;
use crate::types::{Attachment, ConversationInfo, DisplayMessage, ModelInfo};

/// Initialize the application - sets up database and default model
#[tauri::command]
pub async fn init_app(_app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    use noema_core::SqliteStore;

    // Use the same database path as TUI: dirs::data_dir()/noema/conversations.db
    // On mobile, fall back to Tauri's app_data_dir
    #[cfg(not(target_os = "android"))]
    let db_path = {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("noema");
        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir)
                .map_err(|e| format!("Failed to create data dir: {}", e))?;
        }
        data_dir.join("conversations.db")
    };

    #[cfg(target_os = "android")]
    let db_path = {
        let app_dir = _app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {}", e))?;
        if !app_dir.exists() {
            std::fs::create_dir_all(&app_dir)
                .map_err(|e| format!("Failed to create app dir: {}", e))?;
        }
        app_dir.join("conversations.db")
    };

    // Load environment and provider URLs
    config::load_env_file();
    let provider_urls = ProviderUrls::from_env();
    *state.provider_urls.lock().await = provider_urls.clone();

    // Open database
    let store =
        SqliteStore::open(&db_path).map_err(|e| format!("Failed to open database: {}", e))?;

    // Create initial session
    let session = store
        .create_conversation()
        .map_err(|e| format!("Failed to create session: {}", e))?;

    let conversation_id = session.conversation_id().to_string();
    *state.current_conversation_id.lock().await = conversation_id.clone();

    // Create default model (Gemini)
    let provider_type = ModelProviderType::Gemini;
    let (model_id, model_display_name) = get_model_info(&provider_type);
    let provider_instance = create_provider(&provider_type, &provider_urls);
    let model = provider_instance
        .create_chat_model(model_id)
        .ok_or_else(|| format!("Failed to create model: {}", model_id))?;

    *state.model_name.lock().await = model_display_name.to_string();

    // Create MCP registry
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    // Create engine
    let engine = ChatEngine::new(session, model, model_display_name.to_string(), mcp_registry);

    *state.store.lock().await = Some(store);
    *state.engine.lock().await = Some(engine);

    Ok(model_display_name.to_string())
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
        let mime_lower = attachment.mime_type.to_lowercase();
        if mime_lower.starts_with("image/") {
            content.push(ContentBlock::Image {
                data: attachment.data,
                mime_type: attachment.mime_type,
            });
        } else if mime_lower.starts_with("audio/") {
            content.push(ContentBlock::Audio {
                data: attachment.data,
                mime_type: attachment.mime_type,
            });
        }
        // Ignore unsupported types
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
    // Check if already processing - if so, queue this message
    {
        let is_processing = *state.is_processing.lock().await;
        if is_processing {
            // Already processing, the message will be queued in the engine
            // but we shouldn't spawn another polling task
            let engine_guard = state.engine.lock().await;
            let engine = engine_guard.as_ref().ok_or("App not initialized")?;
            engine.send_message(payload);
            return Ok(());
        }
    }

    // Mark as processing
    *state.is_processing.lock().await = true;

    // Emit user message immediately
    let user_msg = DisplayMessage::from_payload(&payload);
    app.emit("user_message", &user_msg)
        .map_err(|e| e.to_string())?;

    // Send to engine
    {
        let engine_guard = state.engine.lock().await;
        let engine = engine_guard.as_ref().ok_or("App not initialized")?;
        engine.send_message(payload);
    }

    // Spawn a task to poll for events
    let app_handle = app.clone();
    tokio::spawn(async move {
        let state = app_handle.state::<AppState>();

        // Use a scope guard pattern to ensure is_processing is always reset
        struct ProcessingGuard<'a> {
            state: &'a AppState,
            completed: bool,
        }

        impl<'a> ProcessingGuard<'a> {
            fn new(state: &'a AppState) -> Self {
                Self { state, completed: false }
            }

            fn mark_completed(&mut self) {
                self.completed = true;
            }
        }

        impl Drop for ProcessingGuard<'_> {
            fn drop(&mut self) {
                if !self.completed {
                    // If we're dropping without completing, reset is_processing
                    // Use blocking lock since we're in drop
                    if let Ok(mut guard) = self.state.is_processing.try_lock() {
                        *guard = false;
                    }
                }
            }
        }

        let mut guard = ProcessingGuard::new(&state);

        loop {
            let event = {
                let mut engine_guard = state.engine.lock().await;
                let engine = match engine_guard.as_mut() {
                    Some(e) => e,
                    None => break,
                };
                engine.try_recv()
            };

            match event {
                Some(EngineEvent::Message(msg)) => {
                    let display_msg = DisplayMessage::from_chat_message(&msg);
                    let _ = app_handle.emit("streaming_message", &display_msg);
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
                    let _ = app_handle.emit("message_complete", &messages);
                    // Mark as no longer processing
                    *state.is_processing.lock().await = false;
                    guard.mark_completed();
                    break;
                }
                Some(EngineEvent::Error(err)) => {
                    let _ = app_handle.emit("error", &err);
                    // Mark as no longer processing
                    *state.is_processing.lock().await = false;
                    guard.mark_completed();
                    break;
                }
                Some(EngineEvent::ModelChanged(name)) => {
                    let _ = app_handle.emit("model_changed", &name);
                }
                Some(EngineEvent::HistoryCleared) => {
                    let _ = app_handle.emit("history_cleared", ());
                }
                None => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
            }
        }
    });

    Ok(())
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
    let provider_type = match provider.to_lowercase().as_str() {
        "ollama" => ModelProviderType::Ollama,
        "gemini" => ModelProviderType::Gemini,
        "claude" => ModelProviderType::Claude,
        "openai" => ModelProviderType::OpenAI,
        _ => return Err(format!("Unknown provider: {}", provider)),
    };

    let provider_urls = state.provider_urls.lock().await.clone();
    let provider_instance = create_provider(&provider_type, &provider_urls);
    let new_model = provider_instance
        .create_chat_model(&model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    let display_name = model_id
        .split('/')
        .last()
        .unwrap_or(&model_id)
        .to_string();

    {
        let mut engine_guard = state.engine.lock().await;
        let engine = engine_guard.as_mut().ok_or("App not initialized")?;
        engine.set_model(new_model, display_name.clone());
    }

    *state.model_name.lock().await = display_name.clone();

    Ok(display_name)
}

/// List available models from all providers
#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let provider_urls = state.provider_urls.lock().await.clone();
    let mut all_models = Vec::new();

    // Gemini
    let gemini = create_provider(&ModelProviderType::Gemini, &provider_urls);
    if let Ok(models) = gemini.list_models().await {
        for m in models {
            all_models.push(ModelInfo {
                id: m.id.clone(),
                display_name: m.name().to_string(),
                provider: "gemini".to_string(),
            });
        }
    }

    // Claude
    let claude = create_provider(&ModelProviderType::Claude, &provider_urls);
    if let Ok(models) = claude.list_models().await {
        for m in models {
            all_models.push(ModelInfo {
                id: m.id.clone(),
                display_name: m.name().to_string(),
                provider: "claude".to_string(),
            });
        }
    }

    // OpenAI
    let openai = create_provider(&ModelProviderType::OpenAI, &provider_urls);
    if let Ok(models) = openai.list_models().await {
        for m in models {
            all_models.push(ModelInfo {
                id: m.id.clone(),
                display_name: m.name().to_string(),
                provider: "openai".to_string(),
            });
        }
    }

    // Ollama
    let ollama = create_provider(&ModelProviderType::Ollama, &provider_urls);
    if let Ok(models) = ollama.list_models().await {
        for m in models {
            all_models.push(ModelInfo {
                id: m.id.clone(),
                display_name: m.name().to_string(),
                provider: "ollama".to_string(),
            });
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

    let provider_urls = state.provider_urls.lock().await.clone();

    let session = {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("App not initialized")?;
        store
            .open_conversation(&conversation_id)
            .map_err(|e| format!("Failed to open conversation: {}", e))?
    };

    let model_name = state.model_name.lock().await.clone();
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    // Create model
    let provider_type = ModelProviderType::Gemini;
    let (model_id, _) = get_model_info(&provider_type);
    let provider_instance = create_provider(&provider_type, &provider_urls);
    let model = provider_instance
        .create_chat_model(model_id)
        .ok_or_else(|| format!("Failed to create model: {}", model_id))?;

    // Get messages before creating new engine
    let messages: Vec<DisplayMessage> = session
        .messages()
        .iter()
        .map(DisplayMessage::from_chat_message)
        .collect();

    let engine = ChatEngine::new(session, model, model_name, mcp_registry);

    *state.engine.lock().await = Some(engine);
    *state.current_conversation_id.lock().await = conversation_id;

    Ok(messages)
}

/// Create a new conversation
#[tauri::command]
pub async fn new_conversation(state: State<'_, AppState>) -> Result<String, String> {
    let provider_urls = state.provider_urls.lock().await.clone();

    let session = {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("App not initialized")?;
        store
            .create_conversation()
            .map_err(|e| format!("Failed to create conversation: {}", e))?
    };

    let conversation_id = session.conversation_id().to_string();
    let model_name = state.model_name.lock().await.clone();
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    let provider_type = ModelProviderType::Gemini;
    let (model_id, _) = get_model_info(&provider_type);
    let provider_instance = create_provider(&provider_type, &provider_urls);
    let model = provider_instance
        .create_chat_model(model_id)
        .ok_or_else(|| format!("Failed to create model: {}", model_id))?;

    let engine = ChatEngine::new(session, model, model_name, mcp_registry);

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
