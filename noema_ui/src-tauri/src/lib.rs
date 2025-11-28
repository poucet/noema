//! Tauri bridge for Noema - connects React frontend to noema-core

use config::{create_provider, get_model_info, ModelProviderType, ProviderUrls};
use llm::{ChatMessage, ChatPayload, ContentBlock, ModelProvider, Role, ToolResultContent};
use noema_core::{ChatEngine, EngineEvent, McpRegistry, SessionStore, SqliteSession, SqliteStore};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

// ============================================================================
// Types for frontend communication
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationInfo {
    pub id: String,
    pub name: Option<String>,
    pub message_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<noema_core::ConversationInfo> for ConversationInfo {
    fn from(info: noema_core::ConversationInfo) -> Self {
        Self {
            id: info.id,
            name: info.name,
            message_count: info.message_count,
            created_at: info.created_at,
            updated_at: info.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayContent {
    Text(String),
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
    ToolCall { name: String, id: String },
    ToolResult { id: String, content: Vec<DisplayToolResultContent> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayToolResultContent {
    Text(String),
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayMessage {
    pub role: String,
    pub content: Vec<DisplayContent>,
}

impl DisplayMessage {
    pub fn from_chat_message(msg: &ChatMessage) -> Self {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        };

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
            role: role.to_string(),
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
            role: "user".to_string(),
            content,
        }
    }
}

// ============================================================================
// Application State
// ============================================================================

pub struct AppState {
    store: Mutex<Option<SqliteStore>>,
    engine: Mutex<Option<ChatEngine<SqliteSession>>>,
    current_conversation_id: Mutex<String>,
    model_name: Mutex<String>,
    provider_urls: Mutex<ProviderUrls>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(None),
            engine: Mutex::new(None),
            current_conversation_id: Mutex::new(String::new()),
            model_name: Mutex::new(String::new()),
            provider_urls: Mutex::new(ProviderUrls::default()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Initialize the application - sets up database and default model
#[tauri::command]
async fn init_app(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
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
        let app_dir = app
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
async fn get_messages(state: State<'_, AppState>) -> Result<Vec<DisplayMessage>, String> {
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
async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
) -> Result<(), String> {
    let payload = ChatPayload::text(message);

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
                    break;
                }
                Some(EngineEvent::Error(err)) => {
                    let _ = app_handle.emit("error", &err);
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
async fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;
    engine.clear_history();
    Ok(())
}

/// Set the current model
#[tauri::command]
async fn set_model(
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
async fn list_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
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
async fn list_conversations(state: State<'_, AppState>) -> Result<Vec<ConversationInfo>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    store
        .list_conversations()
        .map(|convos| convos.into_iter().map(ConversationInfo::from).collect())
        .map_err(|e| format!("Failed to list conversations: {}", e))
}

/// Switch to a different conversation
#[tauri::command]
async fn switch_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<DisplayMessage>, String> {
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
async fn new_conversation(state: State<'_, AppState>) -> Result<String, String> {
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
async fn delete_conversation(
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
async fn rename_conversation(
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
async fn get_model_name(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.model_name.lock().await.clone())
}

/// Get current conversation ID
#[tauri::command]
async fn get_current_conversation_id(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.current_conversation_id.lock().await.clone())
}

// ============================================================================
// Application Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            init_app,
            get_messages,
            send_message,
            clear_history,
            set_model,
            list_models,
            list_conversations,
            switch_conversation,
            new_conversation,
            delete_conversation,
            rename_conversation,
            get_model_name,
            get_current_conversation_id,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
