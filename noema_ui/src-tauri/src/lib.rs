//! Tauri bridge for Noema - connects React frontend to noema-core

use config::{create_provider, get_model_info, ModelProviderType, ProviderUrls};
use llm::{ChatMessage, ChatPayload, ContentBlock, ModelProvider, Role, ToolResultContent};
use noema_audio::{BrowserVoiceSession, VoiceAgent, VoiceCoordinator};
use noema_core::{AuthMethod, ChatEngine, EngineEvent, McpRegistry, ServerConfig, SessionStore, SqliteSession, SqliteStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_deep_link::DeepLinkExt;
use tokio::sync::Mutex;

// ============================================================================
// Logging
// ============================================================================

/// Log a message to ~/Library/Logs/Noema/noema.log
fn log_message(msg: &str) {
    if let Some(log_dir) = dirs::home_dir().map(|h| h.join("Library/Logs/Noema")) {
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("noema.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] {}", timestamp, msg);
        }
    }
}

/// Frontend logging command - allows JS to write to the same log file
#[tauri::command]
fn log_debug(level: String, source: String, message: String) {
    let formatted = format!("[{}] [{}] {}", level.to_uppercase(), source, message);
    log_message(&formatted);
}

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

// MCP server info for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_type: String,
    pub is_connected: bool,
    pub needs_oauth_login: bool,
    pub tool_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub server_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMcpServerRequest {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub use_well_known: bool,
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
    voice_coordinator: Mutex<Option<VoiceCoordinator>>,
    voice_enabled: Mutex<bool>,
    is_processing: Mutex<bool>,
    /// Maps OAuth state parameter to server ID for pending OAuth flows
    pending_oauth_states: Mutex<HashMap<String, String>>,
    /// Browser voice session for WebAudio-based input
    browser_voice_session: Mutex<Option<BrowserVoiceSession>>,
}

impl AppState {
    pub fn new() -> Self {
        // Load pending OAuth states from disk
        let pending_states = load_pending_oauth_states().unwrap_or_default();

        Self {
            store: Mutex::new(None),
            engine: Mutex::new(None),
            current_conversation_id: Mutex::new(String::new()),
            model_name: Mutex::new(String::new()),
            provider_urls: Mutex::new(ProviderUrls::default()),
            voice_coordinator: Mutex::new(None),
            voice_enabled: Mutex::new(false),
            is_processing: Mutex::new(false),
            pending_oauth_states: Mutex::new(pending_states),
            browser_voice_session: Mutex::new(None),
        }
    }
}

/// Get the path to the pending OAuth states file
fn get_oauth_states_path() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| d.join("noema").join("pending_oauth.json"))
}

/// Load pending OAuth states from disk
fn load_pending_oauth_states() -> Option<HashMap<String, String>> {
    let path = get_oauth_states_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save pending OAuth states to disk
fn save_pending_oauth_states(states: &HashMap<String, String>) -> Result<(), String> {
    let path = get_oauth_states_path().ok_or("Could not determine data directory")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string(states).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
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
async fn init_app(_app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
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
    // Check if already processing - if so, queue this message
    {
        let is_processing = *state.is_processing.lock().await;
        if is_processing {
            // Already processing, the message will be queued in the engine
            // but we shouldn't spawn another polling task
            let payload = ChatPayload::text(message);
            let engine_guard = state.engine.lock().await;
            let engine = engine_guard.as_ref().ok_or("App not initialized")?;
            engine.send_message(payload);
            return Ok(());
        }
    }

    // Mark as processing
    *state.is_processing.lock().await = true;

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
                    // Mark as no longer processing
                    *state.is_processing.lock().await = false;
                    break;
                }
                Some(EngineEvent::Error(err)) => {
                    let _ = app_handle.emit("error", &err);
                    // Mark as no longer processing
                    *state.is_processing.lock().await = false;
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
// Voice Commands
// ============================================================================

/// Check if voice is available (Whisper model exists)
#[tauri::command]
async fn is_voice_available() -> Result<bool, String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let data_dir = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let model_path = data_dir.join("noema").join("models").join("ggml-base.en.bin");
        Ok(model_path.exists())
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Ok(false) // Voice not supported on mobile yet
    }
}

/// Get the Whisper model path
fn get_whisper_model_path() -> Option<std::path::PathBuf> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let data_dir = dirs::data_dir()?;
        let model_path = data_dir.join("noema").join("models").join("ggml-base.en.bin");
        if model_path.exists() {
            Some(model_path)
        } else {
            None
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        None
    }
}

/// Toggle voice input on/off
#[tauri::command]
async fn toggle_voice(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let mut voice_enabled = state.voice_enabled.lock().await;

    if *voice_enabled {
        // Disable voice
        *state.voice_coordinator.lock().await = None;
        *voice_enabled = false;
        app.emit("voice_status", "disabled").ok();
        Ok(false)
    } else {
        // Enable voice
        let model_path = get_whisper_model_path()
            .ok_or("Whisper model not found. Please download ggml-base.en.bin to ~/.local/share/noema/models/")?;

        let agent = VoiceAgent::new(&model_path)
            .map_err(|e| format!("Failed to start voice agent: {}", e))?;

        let coordinator = VoiceCoordinator::new(agent);
        *state.voice_coordinator.lock().await = Some(coordinator);
        *voice_enabled = true;

        // Start polling for voice events
        let app_handle = app.clone();
        tokio::spawn(async move {
            let state = app_handle.state::<AppState>();
            loop {
                let voice_enabled = *state.voice_enabled.lock().await;
                if !voice_enabled {
                    break;
                }

                // Check if we're currently processing a message - if so, buffer voice input
                let is_processing = *state.is_processing.lock().await;

                let (messages, errors, is_listening, is_transcribing) = {
                    let mut coordinator_guard = state.voice_coordinator.lock().await;
                    if let Some(coordinator) = coordinator_guard.as_mut() {
                        let is_listening = coordinator.is_listening();
                        let is_transcribing = coordinator.is_transcribing();
                        // Buffer messages while processing, release when not processing
                        let (msgs, errs) = coordinator.process(is_processing);
                        (msgs, errs, is_listening, is_transcribing)
                    } else {
                        break;
                    }
                };

                // Emit status updates
                if is_listening {
                    app_handle.emit("voice_status", "listening").ok();
                } else if is_transcribing {
                    app_handle.emit("voice_status", "transcribing").ok();
                } else {
                    app_handle.emit("voice_status", "enabled").ok();
                }

                // Send transcribed messages as chat messages
                for message in messages {
                    app_handle.emit("voice_transcription", &message).ok();
                }

                // Report errors
                for error in errors {
                    app_handle.emit("voice_error", &error).ok();
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        });

        app.emit("voice_status", "enabled").ok();
        Ok(true)
    }
}

/// Get current voice status
#[tauri::command]
async fn get_voice_status(state: State<'_, AppState>) -> Result<String, String> {
    // Check browser voice session first
    let browser_session = state.browser_voice_session.lock().await;
    if browser_session.is_some() {
        return Ok("listening".to_string());
    }
    drop(browser_session);

    let voice_enabled = *state.voice_enabled.lock().await;
    if !voice_enabled {
        return Ok("disabled".to_string());
    }

    let coordinator_guard = state.voice_coordinator.lock().await;
    if let Some(coordinator) = coordinator_guard.as_ref() {
        if coordinator.is_listening() {
            Ok("listening".to_string())
        } else if coordinator.is_transcribing() {
            Ok("transcribing".to_string())
        } else {
            Ok("enabled".to_string())
        }
    } else {
        Ok("disabled".to_string())
    }
}

// ============================================================================
// Browser Voice Commands (WebAudio-based)
// ============================================================================

/// Start a browser voice session
#[tauri::command]
async fn start_voice_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let model_path = get_whisper_model_path()
        .ok_or("Whisper model not found. Please download ggml-base.en.bin to ~/.local/share/noema/models/")?;

    let session = BrowserVoiceSession::new(&model_path)
        .map_err(|e| format!("Failed to start voice session: {}", e))?;

    *state.browser_voice_session.lock().await = Some(session);
    app.emit("voice_status", "listening").ok();
    log_message("Browser voice session started");

    Ok(())
}

/// Process audio samples from browser WebAudio API
#[tauri::command]
async fn process_audio_chunk(
    app: AppHandle,
    state: State<'_, AppState>,
    samples: Vec<f32>,
) -> Result<(), String> {
    let session_guard = state.browser_voice_session.lock().await;
    let session = session_guard.as_ref().ok_or("No active voice session")?;

    // Process samples through VAD and transcription
    if let Some(transcription) = session.process_samples(&samples) {
        log_message(&format!("Transcription: {}", transcription));
        app.emit("voice_transcription", &transcription).ok();
    }

    // Update status based on speech detection
    if session.is_speech_active() {
        app.emit("voice_status", "listening").ok();
    }

    Ok(())
}

/// Stop the browser voice session and get final transcription
#[tauri::command]
async fn stop_voice_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let mut session_guard = state.browser_voice_session.lock().await;

    if let Some(session) = session_guard.take() {
        app.emit("voice_status", "transcribing").ok();
        log_message("Stopping browser voice session");

        // Get any remaining transcription
        let final_text = session.finish();

        if let Some(ref text) = final_text {
            log_message(&format!("Final transcription: {}", text));
            app.emit("voice_transcription", text).ok();
        }

        app.emit("voice_status", "disabled").ok();
        Ok(final_text)
    } else {
        Ok(None)
    }
}

// ============================================================================
// File Download Commands
// ============================================================================

/// Save binary data to a file using the system save dialog
#[tauri::command]
async fn save_file(
    app: AppHandle,
    data: String,       // base64 encoded data
    filename: String,   // suggested filename
    mime_type: String,  // mime type for file filter
) -> Result<bool, String> {
    use base64::Engine;
    use tauri_plugin_dialog::DialogExt;

    log_message(&format!("save_file called: filename={}, mime_type={}", filename, mime_type));

    // Decode base64 data
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| {
            log_message(&format!("Failed to decode base64: {}", e));
            format!("Failed to decode data: {}", e)
        })?;

    log_message(&format!("Decoded {} bytes", bytes.len()));

    // Determine file extension from mime type
    let extension = mime_type.split('/').nth(1).unwrap_or("bin").to_string();

    // Use a channel to get the result from the dialog callback
    let (tx, rx) = tokio::sync::oneshot::channel();

    app.dialog()
        .file()
        .set_file_name(&filename)
        .add_filter(&mime_type, &[&extension])
        .save_file(move |file_path| {
            let _ = tx.send(file_path);
        });

    let file_path = rx.await.map_err(|e| format!("Dialog error: {}", e))?;

    log_message(&format!("Dialog returned: {:?}", file_path));

    if let Some(path) = file_path {
        let path_buf = path.as_path().ok_or("Invalid path")?;
        log_message(&format!("Writing to: {:?}", path_buf));
        std::fs::write(path_buf, &bytes)
            .map_err(|e| {
                log_message(&format!("Failed to write: {}", e));
                format!("Failed to write file: {}", e)
            })?;
        log_message("File saved successfully");
        Ok(true)
    } else {
        log_message("User cancelled");
        Ok(false) // User cancelled
    }
}

// ============================================================================
// MCP Server Commands
// ============================================================================

/// List all configured MCP servers
#[tauri::command]
async fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerInfo>, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let registry = mcp_registry.lock().await;

    let mut servers = Vec::new();
    for (id, config) in registry.list_servers() {
        let is_connected = registry.is_connected(id);
        let tool_count = if let Some(conn) = registry.get_connection(id) {
            conn.tools.len()
        } else {
            0
        };

        let auth_type = match &config.auth {
            AuthMethod::None => "none",
            AuthMethod::Token { .. } => "token",
            AuthMethod::OAuth { .. } => "oauth",
        };

        servers.push(McpServerInfo {
            id: id.to_string(),
            name: config.name.clone(),
            url: config.url.clone(),
            auth_type: auth_type.to_string(),
            is_connected,
            needs_oauth_login: config.auth.needs_oauth_login(),
            tool_count,
        });
    }

    Ok(servers)
}

/// Add a new MCP server configuration
/// If auth_type is not specified or is "auto", probe .well-known to detect OAuth
#[tauri::command]
async fn add_mcp_server(
    state: State<'_, AppState>,
    request: AddMcpServerRequest,
) -> Result<(), String> {
    let auth = match request.auth_type.as_str() {
        "token" => AuthMethod::Token {
            token: request.token.ok_or("Token required for token auth")?,
        },
        "oauth" => {
            // Explicitly requested OAuth
            AuthMethod::OAuth {
                client_id: request.client_id.unwrap_or_else(|| "noema".to_string()),
                client_secret: request.client_secret,
                authorization_url: None,
                token_url: None,
                scopes: request.scopes,
                access_token: None,
                refresh_token: None,
                expires_at: None,
            }
        }
        "none" => AuthMethod::None,
        _ => {
            // Auto-detect: probe .well-known to see if OAuth is available
            log_message(&format!("Auto-detecting auth for server: {}", request.url));
            if let Ok(well_known) = fetch_well_known(&request.url).await {
                if well_known.get("authorization_endpoint").is_some() {
                    log_message("OAuth detected via .well-known");
                    AuthMethod::OAuth {
                        client_id: "noema".to_string(),
                        client_secret: None,
                        authorization_url: None,
                        token_url: None,
                        scopes: vec![],
                        access_token: None,
                        refresh_token: None,
                        expires_at: None,
                    }
                } else {
                    log_message("No OAuth in .well-known, using no auth");
                    AuthMethod::None
                }
            } else {
                log_message("No .well-known found, using no auth");
                AuthMethod::None
            }
        }
    };

    let use_well_known = matches!(auth, AuthMethod::OAuth { .. });

    let config = ServerConfig {
        name: request.name,
        url: request.url,
        auth,
        use_well_known,
        auth_token: None,
    };

    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let mut registry = mcp_registry.lock().await;
    registry.add_server(request.id, config);
    registry.save_config().map_err(|e| e.to_string())?;

    Ok(())
}

/// Remove an MCP server configuration
#[tauri::command]
async fn remove_mcp_server(state: State<'_, AppState>, server_id: String) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let mut registry = mcp_registry.lock().await;
    registry
        .remove_server(&server_id)
        .await
        .map_err(|e| e.to_string())?;
    registry.save_config().map_err(|e| e.to_string())?;

    Ok(())
}

/// Connect to an MCP server
#[tauri::command]
async fn connect_mcp_server(state: State<'_, AppState>, server_id: String) -> Result<usize, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let mut registry = mcp_registry.lock().await;

    let server = registry
        .connect(&server_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(server.tools.len())
}

/// Disconnect from an MCP server
#[tauri::command]
async fn disconnect_mcp_server(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let mut registry = mcp_registry.lock().await;
    registry
        .disconnect(&server_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Get tools from a connected MCP server
#[tauri::command]
async fn get_mcp_server_tools(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpToolInfo>, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let registry = mcp_registry.lock().await;

    let server = registry
        .get_connection(&server_id)
        .ok_or("Server not connected")?;

    let tools = server
        .tools
        .iter()
        .map(|tool| McpToolInfo {
            name: tool.name.to_string(),
            description: tool.description.as_ref().map(|d| d.to_string()),
            server_id: server_id.clone(),
        })
        .collect();

    Ok(tools)
}

/// Test connection to an MCP server (connect and immediately disconnect)
#[tauri::command]
async fn test_mcp_server(state: State<'_, AppState>, server_id: String) -> Result<usize, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let mut registry = mcp_registry.lock().await;

    // Connect to test
    let server = registry
        .connect(&server_id)
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    let tool_count = server.tools.len();

    // Disconnect after test
    registry
        .disconnect(&server_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(tool_count)
}

/// Fetch .well-known OAuth configuration
async fn fetch_well_known(base_url: &str) -> Result<serde_json::Value, String> {
    let base = url::Url::parse(base_url).map_err(|e| format!("Invalid server URL: {}", e))?;
    let well_known_url = base
        .join("/.well-known/oauth-authorization-server")
        .map_err(|e| format!("Failed to construct well-known URL: {}", e))?;

    let client = reqwest::Client::new();
    let resp = client
        .get(well_known_url.as_str())
        .send()
        .await
        .map_err(|e| format!("Failed to fetch well-known config: {}", e))?;

    resp.json()
        .await
        .map_err(|e| format!("Failed to parse well-known config: {}", e))
}

/// Perform dynamic client registration (RFC 7591)
async fn register_oauth_client(
    registration_endpoint: &str,
    redirect_uri: &str,
) -> Result<(String, Option<String>), String> {
    let client = reqwest::Client::new();

    let registration_request = serde_json::json!({
        "client_name": "Noema",
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none"  // Public client
    });

    let resp = client
        .post(registration_endpoint)
        .json(&registration_request)
        .send()
        .await
        .map_err(|e| format!("Client registration failed: {}", e))?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        return Err(format!("Client registration failed: {}", error_text));
    }

    let registration_response: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse registration response: {}", e))?;

    let client_id = registration_response["client_id"]
        .as_str()
        .ok_or("No client_id in registration response")?
        .to_string();

    let client_secret = registration_response["client_secret"].as_str().map(String::from);

    Ok((client_id, client_secret))
}

/// Start OAuth flow for an MCP server (returns authorization URL)
#[tauri::command]
async fn start_mcp_oauth(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<String, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let mut registry = mcp_registry.lock().await;

    let config = registry
        .config()
        .get_server(&server_id)
        .ok_or("Server not found")?
        .clone();

    match &config.auth {
        AuthMethod::OAuth {
            client_id,
            client_secret,
            authorization_url,
            scopes,
            ..
        } => {
            let redirect_uri = "noema://oauth/callback";

            // Fetch .well-known config if needed
            let well_known = if config.use_well_known {
                Some(fetch_well_known(&config.url).await?)
            } else {
                None
            };

            // Get authorization URL
            let auth_url = if let Some(url) = authorization_url {
                url.clone()
            } else if let Some(ref wk) = well_known {
                wk["authorization_endpoint"]
                    .as_str()
                    .ok_or("No authorization_endpoint in well-known config")?
                    .to_string()
            } else {
                return Err("OAuth requires authorization_url or use_well_known".to_string());
            };

            // Check if we need to register the client dynamically.
            // We always re-register if client_id is "noema", empty, or if there's no access_token yet
            // (which means a previous registration may have used a different redirect_uri).
            let needs_registration = client_id == "noema" || client_id.is_empty();

            let (final_client_id, _final_client_secret) = if needs_registration {
                // Try dynamic client registration
                if let Some(ref wk) = well_known {
                    if let Some(reg_endpoint) = wk["registration_endpoint"].as_str() {
                        let (new_id, new_secret) =
                            register_oauth_client(reg_endpoint, redirect_uri).await?;

                        // Update config with new client credentials
                        let updated_auth = AuthMethod::OAuth {
                            client_id: new_id.clone(),
                            client_secret: new_secret.clone(),
                            authorization_url: Some(auth_url.clone()),
                            token_url: wk["token_endpoint"].as_str().map(String::from),
                            scopes: scopes.clone(),
                            access_token: None,
                            refresh_token: None,
                            expires_at: None,
                        };

                        let updated_config = ServerConfig {
                            name: config.name.clone(),
                            url: config.url.clone(),
                            auth: updated_auth,
                            use_well_known: config.use_well_known,
                            auth_token: None,
                        };

                        registry.add_server(server_id.clone(), updated_config);
                        registry.save_config().map_err(|e| e.to_string())?;

                        (new_id, new_secret)
                    } else {
                        return Err("Server requires client registration but no registration_endpoint found. Please configure client_id manually.".to_string());
                    }
                } else {
                    return Err("Cannot register client without .well-known discovery".to_string());
                }
            } else {
                (client_id.clone(), client_secret.clone())
            };

            // Build authorization URL with state parameter that maps to server_id
            let state_param = uuid::Uuid::new_v4().to_string();

            // Store the state -> server_id mapping in memory and persist to disk
            {
                let mut pending_states = state.pending_oauth_states.lock().await;
                pending_states.insert(state_param.clone(), server_id.clone());
                // Persist to disk so it survives app restart
                if let Err(e) = save_pending_oauth_states(&pending_states) {
                    log_message(&format!("Warning: Failed to persist OAuth state: {}", e));
                }
            }

            let scope_str = if scopes.is_empty() {
                "openid".to_string()
            } else {
                scopes.join(" ")
            };

            let mut url = url::Url::parse(&auth_url)
                .map_err(|e| format!("Invalid authorization URL: {}", e))?;

            url.query_pairs_mut()
                .append_pair("client_id", &final_client_id)
                .append_pair("response_type", "code")
                .append_pair("redirect_uri", redirect_uri)
                .append_pair("state", &state_param)
                .append_pair("scope", &scope_str);

            Ok(url.to_string())
        }
        _ => Err("Server is not configured for OAuth".to_string()),
    }
}

/// Complete OAuth flow with authorization code
#[tauri::command]
async fn complete_mcp_oauth(
    state: State<'_, AppState>,
    server_id: String,
    code: String,
) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let mut registry = mcp_registry.lock().await;

    let config = registry
        .config()
        .get_server(&server_id)
        .ok_or("Server not found")?
        .clone();

    match &config.auth {
        AuthMethod::OAuth {
            client_id,
            client_secret,
            token_url,
            authorization_url,
            scopes,
            ..
        } => {
            // Get token URL
            let tok_url = if let Some(url) = token_url {
                url.clone()
            } else if config.use_well_known {
                let well_known = fetch_well_known(&config.url).await?;
                well_known["token_endpoint"]
                    .as_str()
                    .ok_or("No token_endpoint in well-known config")?
                    .to_string()
            } else {
                return Err("OAuth requires token_url or use_well_known".to_string());
            };

            // Use same redirect_uri as in start_mcp_oauth
            let redirect_uri = "noema://oauth/callback";
            let http_client = reqwest::Client::new();

            let mut params = vec![
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", redirect_uri),
                ("client_id", client_id.as_str()),
            ];

            let client_secret_str;
            if let Some(secret) = client_secret {
                client_secret_str = secret.clone();
                params.push(("client_secret", &client_secret_str));
            }

            let resp = http_client
                .post(&tok_url)
                .form(&params)
                .send()
                .await
                .map_err(|e| format!("Token exchange failed: {}", e))?;

            if !resp.status().is_success() {
                let error_text = resp.text().await.unwrap_or_default();
                return Err(format!("Token exchange failed: {}", error_text));
            }

            let token_response: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse token response: {}", e))?;

            let access_token = token_response["access_token"]
                .as_str()
                .ok_or("No access_token in response")?
                .to_string();

            let refresh_token = token_response["refresh_token"].as_str().map(String::from);

            let expires_in = token_response["expires_in"].as_i64();
            let expires_at = expires_in.map(|exp| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
                    + exp
            });

            // Update the server config with tokens
            let updated_auth = AuthMethod::OAuth {
                client_id: client_id.clone(),
                client_secret: client_secret.clone(),
                authorization_url: authorization_url.clone(),
                token_url: Some(tok_url),
                scopes: scopes.clone(),
                access_token: Some(access_token),
                refresh_token,
                expires_at,
            };

            let updated_config = ServerConfig {
                name: config.name.clone(),
                url: config.url.clone(),
                auth: updated_auth,
                use_well_known: config.use_well_known,
                auth_token: None,
            };

            registry.add_server(server_id, updated_config);
            registry.save_config().map_err(|e| e.to_string())?;

            Ok(())
        }
        _ => Err("Server is not configured for OAuth".to_string()),
    }
}

// ============================================================================
// Deep Link Handler
// ============================================================================

/// Handle incoming deep link URLs (e.g., noema://oauth/callback?code=...&state=...)
async fn handle_deep_link(app: &AppHandle, urls: Vec<url::Url>) {
    for url in urls {
        log_message(&format!("Deep link received: {}", url));

        // Check if this is an OAuth callback
        // Note: In noema://oauth/callback, "oauth" is the host and "/callback" is the path
        let is_oauth_callback = url.scheme() == "noema"
            && url.host_str() == Some("oauth")
            && url.path() == "/callback";

        if is_oauth_callback {
            // Extract the code and state from query params
            let code = url.query_pairs()
                .find(|(key, _)| key == "code")
                .map(|(_, value)| value.to_string());

            let state_param = url.query_pairs()
                .find(|(key, _)| key == "state")
                .map(|(_, value)| value.to_string());

            if let (Some(auth_code), Some(oauth_state)) = (code.as_ref(), state_param.as_ref()) {
                let app_state = app.state::<AppState>();

                // Look up server ID from state parameter
                let server_id = {
                    let mut pending_states = app_state.pending_oauth_states.lock().await;
                    let server_id = pending_states.remove(oauth_state);

                    // Update persisted state
                    if server_id.is_some() {
                        if let Err(e) = save_pending_oauth_states(&pending_states) {
                            log_message(&format!("Warning: Failed to update persisted OAuth state: {}", e));
                        }
                    }

                    server_id
                };

                log_message(&format!("Found server_id for state: {:?}", server_id));

                if let Some(server_id) = server_id {
                    // Complete OAuth flow
                    match complete_oauth_internal(app, &server_id, &auth_code).await {
                        Ok(()) => {
                            log_message(&format!("OAuth completed successfully for server: {}", server_id));
                            // Emit success event to frontend
                            app.emit("oauth_complete", &server_id).ok();
                        }
                        Err(e) => {
                            log_message(&format!("OAuth error: {}", e));
                            // Emit error event to frontend
                            app.emit("oauth_error", &e).ok();
                        }
                    }
                } else {
                    let err = format!("No pending OAuth flow found for state: {}", oauth_state);
                    log_message(&err);
                    app.emit("oauth_error", &err).ok();
                }
            } else {
                // Missing code or state - log but don't emit error (may be duplicate/incomplete callback)
                log_message(&format!("Incomplete OAuth callback - code: {:?}, state: {:?}", code.is_some(), state_param.is_some()));
            }
        }
    }
}

/// Internal function to complete OAuth (shared by command and deep link handler)
async fn complete_oauth_internal(
    app: &AppHandle,
    server_id: &str,
    code: &str,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let mut registry = mcp_registry.lock().await;

    let config = registry
        .config()
        .get_server(server_id)
        .ok_or("Server not found")?
        .clone();

    match &config.auth {
        AuthMethod::OAuth {
            client_id,
            client_secret,
            token_url,
            authorization_url,
            scopes,
            ..
        } => {
            // Get token URL
            let tok_url = if let Some(url) = token_url {
                url.clone()
            } else if config.use_well_known {
                let well_known = fetch_well_known(&config.url).await?;
                well_known["token_endpoint"]
                    .as_str()
                    .ok_or("No token_endpoint in well-known config")?
                    .to_string()
            } else {
                return Err("OAuth requires token_url or use_well_known".to_string());
            };

            let redirect_uri = "noema://oauth/callback";
            let http_client = reqwest::Client::new();

            let mut params = vec![
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", client_id.as_str()),
            ];

            let client_secret_str;
            if let Some(secret) = client_secret {
                client_secret_str = secret.clone();
                params.push(("client_secret", &client_secret_str));
            }

            let resp = http_client
                .post(&tok_url)
                .form(&params)
                .send()
                .await
                .map_err(|e| format!("Token exchange failed: {}", e))?;

            if !resp.status().is_success() {
                let error_text = resp.text().await.unwrap_or_default();
                return Err(format!("Token exchange failed: {}", error_text));
            }

            let token_response: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse token response: {}", e))?;

            let access_token = token_response["access_token"]
                .as_str()
                .ok_or("No access_token in response")?
                .to_string();

            let refresh_token = token_response["refresh_token"].as_str().map(String::from);

            let expires_in = token_response["expires_in"].as_i64();
            let expires_at = expires_in.map(|exp| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
                    + exp
            });

            // Update the server config with tokens
            let updated_auth = AuthMethod::OAuth {
                client_id: client_id.clone(),
                client_secret: client_secret.clone(),
                authorization_url: authorization_url.clone(),
                token_url: Some(tok_url),
                scopes: scopes.clone(),
                access_token: Some(access_token),
                refresh_token,
                expires_at,
            };

            let updated_config = ServerConfig {
                name: config.name.clone(),
                url: config.url.clone(),
                auth: updated_auth,
                use_well_known: config.use_well_known,
                auth_token: None,
            };

            registry.add_server(server_id.to_string(), updated_config);
            registry.save_config().map_err(|e| e.to_string())?;

            Ok(())
        }
        _ => Err("Server is not configured for OAuth".to_string()),
    }
}

// ============================================================================
// Application Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // When a second instance is launched, this callback receives the args
            // Check if any arg is a deep link URL
            log_message(&format!("Single instance callback, argv: {:?}", argv));
            for arg in argv {
                if arg.starts_with("noema://") {
                    if let Ok(url) = url::Url::parse(&arg) {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            handle_deep_link(&handle, vec![url]).await;
                        });
                    }
                }
            }
            // Focus the existing window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .manage(AppState::new())
        .setup(|app| {
            // Register deep link handler for when app is already running
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                let urls = event.urls();
                log_message(&format!("Deep link on_open_url, urls: {:?}", urls));
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    handle_deep_link(&handle, urls).await;
                });
            });
            Ok(())
        })
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
            is_voice_available,
            toggle_voice,
            get_voice_status,
            // Browser voice commands
            start_voice_session,
            process_audio_chunk,
            stop_voice_session,
            // File commands
            save_file,
            // Logging
            log_debug,
            // MCP server commands
            list_mcp_servers,
            add_mcp_server,
            remove_mcp_server,
            connect_mcp_server,
            disconnect_mcp_server,
            get_mcp_server_tools,
            test_mcp_server,
            start_mcp_oauth,
            complete_mcp_oauth,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
