//! Noema GUI - Native Bevy/egui wrapper for the Noema LLM client
//!
//! Architecture:
//! - Main thread (Bevy): Windowing, input, rendering, egui layout, voice input
//! - Background thread (Tokio): HTTP, MCP, database I/O, OAuth
//!
//! Communication via crossbeam channels (AppCommand -> Core, CoreEvent -> UI)

use base64::Engine;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::collections::HashMap;
use std::path::PathBuf;

use config::{create_provider, get_model_info, load_env_file, ModelProviderType, ProviderUrls};
use llm::{ChatMessage, ChatPayload, ContentBlock, ModelProvider, Role, ToolResultContent};
use noema_audio::{AudioPlayback, VoiceAgent, VoiceCoordinator};
use noema_core::{
    ChatEngine, ConversationInfo, EngineEvent, McpRegistry, SessionStore, SqliteSession,
    SqliteStore,
};

// ============================================================================
// Commands and Events for async bridge
// ============================================================================

/// Commands from UI to Core (async backend)
pub enum AppCommand {
    SendMessage(ChatPayload),
    LoadHistory,
    ClearHistory,
    SetModel { provider: String, model: Option<String> },
    ListConversations,
    SwitchConversation(String),
    NewConversation,
    DeleteConversation(String),
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
    fn from_chat_message(msg: &ChatMessage) -> Self {
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

    fn from_payload(payload: &ChatPayload) -> Self {
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

// ============================================================================
// Bevy Resources
// ============================================================================

/// Holds channels for communicating with the async backend
#[derive(Resource)]
struct CoreConnection {
    cmd_tx: Sender<AppCommand>,
    event_rx: Receiver<CoreEvent>,
}

/// Available model providers
const PROVIDERS: &[(&str, &str)] = &[
    ("gemini", "Gemini"),
    ("claude", "Claude"),
    ("openai", "OpenAI"),
    ("ollama", "Ollama"),
];

/// UI state resource
#[derive(Resource)]
struct UiState {
    input_text: String,
    messages: Vec<DisplayMessage>,
    /// Streaming messages (multimodal) being received
    streaming_messages: Vec<DisplayMessage>,
    is_streaming: bool,
    status_message: Option<String>,
    model_name: String,
    scroll_to_bottom: bool,
    // Pending image attachments (base64, mime_type)
    pending_images: Vec<(String, String)>,
    // Voice state
    voice_enabled: bool,
    voice_listening: bool,
    voice_transcribing: bool,
    // Side panel state
    side_panel_open: bool,
    conversations: Vec<ConversationInfo>,
    current_conversation_id: Option<String>,
    // Model selection
    selected_provider_idx: usize,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            messages: Vec::new(),
            streaming_messages: Vec::new(),
            is_streaming: false,
            status_message: None,
            model_name: "Loading...".to_string(),
            scroll_to_bottom: true,
            pending_images: Vec::new(),
            voice_enabled: false,
            voice_listening: false,
            voice_transcribing: false,
            side_panel_open: false,
            conversations: Vec::new(),
            current_conversation_id: None,
            selected_provider_idx: 0, // Default to Gemini
        }
    }
}

/// Scale factor for UI (1.0 for Mac, 2.5 for Android)
#[derive(Resource)]
struct UiScale(f32);

impl Default for UiScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Voice coordinator resource (optional - only present if voice is enabled)
#[derive(Resource)]
struct VoiceState {
    coordinator: Option<VoiceCoordinator>,
    whisper_model_path: PathBuf,
}

/// Audio playback resource for playing audio content
#[derive(Resource)]
struct AudioPlayer {
    playback: Option<AudioPlayback>,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self {
            playback: AudioPlayback::new().ok(),
        }
    }
}

/// Cache for decoded images (base64 hash -> egui TextureHandle)
#[derive(Resource, Default)]
struct ImageCache {
    textures: HashMap<u64, egui::TextureHandle>,
}

/// Track which audio clips are currently playing
#[derive(Resource, Default)]
struct AudioPlayState {
    playing: std::collections::HashSet<u64>,
}

// ============================================================================
// Async Backend Runner
// ============================================================================

fn spawn_async_backend(
    cmd_rx: Receiver<AppCommand>,
    event_tx: Sender<CoreEvent>,
    provider_urls: ProviderUrls,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async move {
            run_backend(cmd_rx, event_tx, provider_urls).await;
        });
    });
}

async fn run_backend(
    cmd_rx: Receiver<AppCommand>,
    event_tx: Sender<CoreEvent>,
    provider_urls: ProviderUrls,
) {
    // Initialize database
    let db_path = get_db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let store = match SqliteStore::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            let _ = event_tx.send(CoreEvent::Error(format!("Failed to open database: {}", e)));
            return;
        }
    };

    let session = match store.create_conversation() {
        Ok(s) => s,
        Err(e) => {
            let _ = event_tx.send(CoreEvent::Error(format!("Failed to create session: {}", e)));
            return;
        }
    };

    // Create default model (Gemini)
    let provider_type = ModelProviderType::Gemini;
    let (model_id, model_display_name) = get_model_info(&provider_type);
    let provider_instance = create_provider(&provider_type, &provider_urls);
    let model = match provider_instance.create_chat_model(model_id) {
        Some(m) => m,
        None => {
            let _ = event_tx.send(CoreEvent::Error(format!(
                "Failed to create model: {}",
                model_id
            )));
            return;
        }
    };

    let _ = event_tx.send(CoreEvent::ModelChanged(model_display_name.to_string()));

    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));
    let mut engine: ChatEngine<SqliteSession> =
        ChatEngine::new(session, model, model_display_name.to_string(), mcp_registry);

    // Track current conversation ID
    let mut current_conversation_id = {
        let session_arc = engine.get_session();
        let id = if let Ok(sess) = session_arc.try_lock() {
            sess.conversation_id().to_string()
        } else {
            String::new()
        };
        id
    };

    // Load initial history
    {
        let session_arc = engine.get_session();
        if let Ok(sess) = session_arc.try_lock() {
            let messages: Vec<DisplayMessage> = sess
                .messages()
                .iter()
                .map(DisplayMessage::from_chat_message)
                .collect();
            let _ = event_tx.send(CoreEvent::HistoryLoaded(messages));
        };
    }

    // Send initial conversations list
    if let Ok(convos) = store.list_conversations() {
        let _ = event_tx.send(CoreEvent::ConversationsList(convos));
    }

    // Main command loop
    loop {
        // Check for commands (non-blocking)
        if let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                AppCommand::SendMessage(payload) => {
                    // Send user message event immediately so UI shows it
                    let display_msg = DisplayMessage::from_payload(&payload);
                    let _ = event_tx.send(CoreEvent::UserMessageSent(display_msg));

                    engine.send_message(payload);
                }
                AppCommand::ClearHistory => {
                    engine.clear_history();
                }
                AppCommand::LoadHistory => {
                    let session_arc = engine.get_session();
                    if let Ok(sess) = session_arc.try_lock() {
                        let messages: Vec<DisplayMessage> = sess
                            .messages()
                            .iter()
                            .map(DisplayMessage::from_chat_message)
                            .collect();
                        let _ = event_tx.send(CoreEvent::HistoryLoaded(messages));
                    };
                }
                AppCommand::SetModel { provider, model } => {
                    let provider_type = match provider.to_lowercase().as_str() {
                        "ollama" => ModelProviderType::Ollama,
                        "gemini" => ModelProviderType::Gemini,
                        "claude" => ModelProviderType::Claude,
                        "openai" => ModelProviderType::OpenAI,
                        _ => {
                            let _ = event_tx
                                .send(CoreEvent::Error(format!("Unknown provider: {}", provider)));
                            continue;
                        }
                    };
                    let (default_model_id, model_display_name) = get_model_info(&provider_type);
                    let provider_instance = create_provider(&provider_type, &provider_urls);
                    let model_id = model.as_deref().unwrap_or(default_model_id);
                    if let Some(new_model) = provider_instance.create_chat_model(model_id) {
                        engine.set_model(new_model, model_display_name.to_string());
                        let _ =
                            event_tx.send(CoreEvent::ModelChanged(model_display_name.to_string()));
                    } else {
                        let _ = event_tx
                            .send(CoreEvent::Error(format!("Model not found: {}", model_id)));
                    }
                }
                AppCommand::ListConversations => {
                    if let Ok(convos) = store.list_conversations() {
                        let _ = event_tx.send(CoreEvent::ConversationsList(convos));
                    }
                }
                AppCommand::SwitchConversation(id) => {
                    match store.open_conversation(&id) {
                        Ok(session) => {
                            // Get current model info from engine
                            let model_name = engine.get_model_name().to_string();
                            let mcp_registry = McpRegistry::load()
                                .unwrap_or_else(|_| McpRegistry::new(Default::default()));

                            // Recreate provider to get model
                            // For now, just use Gemini as default when switching
                            let provider_type = ModelProviderType::Gemini;
                            let (model_id, _) = get_model_info(&provider_type);
                            let provider_instance = create_provider(&provider_type, &provider_urls);
                            if let Some(model) = provider_instance.create_chat_model(model_id) {
                                engine = ChatEngine::new(session, model, model_name, mcp_registry);
                                current_conversation_id = id.clone();

                                // Load messages for this conversation
                                let session_arc = engine.get_session();
                                if let Ok(sess) = session_arc.try_lock() {
                                    let messages: Vec<DisplayMessage> = sess
                                        .messages()
                                        .iter()
                                        .map(DisplayMessage::from_chat_message)
                                        .collect();
                                    let _ = event_tx.send(CoreEvent::HistoryLoaded(messages));
                                };
                                let _ = event_tx.send(CoreEvent::ConversationSwitched(id));
                            }
                        }
                        Err(e) => {
                            let _ = event_tx.send(CoreEvent::Error(format!(
                                "Failed to open conversation: {}",
                                e
                            )));
                        }
                    }
                }
                AppCommand::NewConversation => {
                    match store.create_conversation() {
                        Ok(session) => {
                            let model_name = engine.get_model_name().to_string();
                            let mcp_registry = McpRegistry::load()
                                .unwrap_or_else(|_| McpRegistry::new(Default::default()));

                            let provider_type = ModelProviderType::Gemini;
                            let (model_id, _) = get_model_info(&provider_type);
                            let provider_instance = create_provider(&provider_type, &provider_urls);
                            if let Some(model) = provider_instance.create_chat_model(model_id) {
                                let new_id = {
                                    // Get the new conversation ID before we move the session
                                    session.conversation_id().to_string()
                                };
                                engine = ChatEngine::new(session, model, model_name, mcp_registry);
                                current_conversation_id = new_id.clone();

                                let _ = event_tx.send(CoreEvent::HistoryLoaded(vec![]));
                                let _ = event_tx.send(CoreEvent::ConversationCreated(new_id));

                                // Refresh conversations list
                                if let Ok(convos) = store.list_conversations() {
                                    let _ = event_tx.send(CoreEvent::ConversationsList(convos));
                                }
                            }
                        }
                        Err(e) => {
                            let _ = event_tx.send(CoreEvent::Error(format!(
                                "Failed to create conversation: {}",
                                e
                            )));
                        }
                    }
                }
                AppCommand::DeleteConversation(id) => {
                    // Don't delete current conversation
                    if id == current_conversation_id {
                        let _ = event_tx
                            .send(CoreEvent::Error("Cannot delete current conversation".into()));
                        continue;
                    }
                    if let Err(e) = store.delete_conversation(&id) {
                        let _ = event_tx
                            .send(CoreEvent::Error(format!("Failed to delete: {}", e)));
                    } else {
                        // Refresh list
                        if let Ok(convos) = store.list_conversations() {
                            let _ = event_tx.send(CoreEvent::ConversationsList(convos));
                        }
                    }
                }
            }
        }

        // Check for engine events (non-blocking)
        while let Some(engine_event) = engine.try_recv() {
            match engine_event {
                EngineEvent::Message(msg) => {
                    let display_msg = DisplayMessage::from_chat_message(&msg);
                    let _ = event_tx.send(CoreEvent::StreamingMessage(display_msg));
                }
                EngineEvent::MessageComplete => {
                    // Reload messages from session after completion
                    let session_arc = engine.get_session();
                    if let Ok(sess) = session_arc.try_lock() {
                        let messages: Vec<DisplayMessage> = sess
                            .messages()
                            .iter()
                            .map(DisplayMessage::from_chat_message)
                            .collect();
                        let _ = event_tx.send(CoreEvent::HistoryLoaded(messages));
                    };
                    let _ = event_tx.send(CoreEvent::MessageComplete);

                    // Refresh conversations list (message count may have changed)
                    if let Ok(convos) = store.list_conversations() {
                        let _ = event_tx.send(CoreEvent::ConversationsList(convos));
                    }
                }
                EngineEvent::Error(err) => {
                    let _ = event_tx.send(CoreEvent::Error(err));
                }
                EngineEvent::ModelChanged(name) => {
                    let _ = event_tx.send(CoreEvent::ModelChanged(name));
                }
                EngineEvent::HistoryCleared => {
                    let _ = event_tx.send(CoreEvent::HistoryCleared);
                }
            }
        }

        // Small sleep to avoid busy-waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}

fn get_db_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("noema")
            .join("conversations.db")
    }

    #[cfg(target_os = "android")]
    {
        PathBuf::from("/data/data/com.noema.app/databases/conversations.db")
    }

    #[cfg(not(any(target_os = "macos", target_os = "android")))]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("noema")
            .join("conversations.db")
    }
}

fn get_whisper_model_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("noema")
        .join("models")
        .join("ggml-base.en.bin")
}

// ============================================================================
// Bevy Systems
// ============================================================================

/// System to read events from the Core and update UI state
fn event_reader_system(connection: Res<CoreConnection>, mut ui_state: ResMut<UiState>) {
    while let Ok(event) = connection.event_rx.try_recv() {
        match event {
            CoreEvent::HistoryLoaded(messages) => {
                ui_state.messages = messages;
                ui_state.scroll_to_bottom = true;
            }
            CoreEvent::MessageReceived(msg) => {
                ui_state.messages.push(msg);
                ui_state.scroll_to_bottom = true;
            }
            CoreEvent::UserMessageSent(msg) => {
                // Immediately show user message
                ui_state.messages.push(msg);
                ui_state.scroll_to_bottom = true;
                ui_state.is_streaming = true;
            }
            CoreEvent::StreamingMessage(msg) => {
                ui_state.streaming_messages.push(msg);
                ui_state.is_streaming = true;
            }
            CoreEvent::MessageComplete => {
                ui_state.streaming_messages.clear();
                ui_state.is_streaming = false;
                ui_state.scroll_to_bottom = true;
            }
            CoreEvent::Error(err) => {
                ui_state.status_message = Some(format!("Error: {}", err));
                ui_state.is_streaming = false;
            }
            CoreEvent::ModelChanged(name) => {
                ui_state.model_name = name;
                ui_state.status_message = None;
            }
            CoreEvent::HistoryCleared => {
                ui_state.messages.clear();
                ui_state.streaming_messages.clear();
                ui_state.status_message = Some("History cleared".to_string());
            }
            CoreEvent::ConversationsList(convos) => {
                ui_state.conversations = convos;
            }
            CoreEvent::ConversationSwitched(id) => {
                ui_state.current_conversation_id = Some(id);
                ui_state.streaming_messages.clear();
                ui_state.is_streaming = false;
            }
            CoreEvent::ConversationCreated(id) => {
                ui_state.current_conversation_id = Some(id);
            }
        }
    }
}

/// System to process voice events
fn voice_system(
    mut voice_state: ResMut<VoiceState>,
    mut ui_state: ResMut<UiState>,
    connection: Res<CoreConnection>,
) {
    if let Some(ref mut coordinator) = voice_state.coordinator {
        // Update voice state flags
        ui_state.voice_listening = coordinator.is_listening();
        ui_state.voice_transcribing = coordinator.is_transcribing();

        // Process voice events - buffer if streaming
        let (messages, errors) = coordinator.process(ui_state.is_streaming);

        // Send transcribed messages
        for msg in messages {
            if !msg.trim().is_empty() {
                let _ = connection
                    .cmd_tx
                    .send(AppCommand::SendMessage(ChatPayload::text(msg)));
                ui_state.is_streaming = true;
                ui_state.scroll_to_bottom = true;
            }
        }

        // Report errors
        for err in errors {
            ui_state.status_message = Some(format!("Voice error: {}", err));
        }
    }
}

/// One-time setup system for egui styling
fn setup_egui(mut contexts: EguiContexts, ui_scale: Res<UiScale>) {
    let ctx = contexts.ctx_mut();
    ctx.set_pixels_per_point(ui_scale.0);
}

/// Handle file drops for image attachments
fn file_drop_system(
    mut ui_state: ResMut<UiState>,
    mut file_drag_and_drop_events: EventReader<bevy::window::FileDragAndDrop>,
) {
    for event in file_drag_and_drop_events.read() {
        if let bevy::window::FileDragAndDrop::DroppedFile { path_buf, .. } = event {
            let ext = path_buf
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let mime_type = match ext.as_str() {
                "png" => Some("image/png"),
                "jpg" | "jpeg" => Some("image/jpeg"),
                "gif" => Some("image/gif"),
                "webp" => Some("image/webp"),
                _ => None,
            };

            if let Some(mime) = mime_type {
                if let Ok(data) = std::fs::read(path_buf) {
                    let base64_data = base64::engine::general_purpose::STANDARD.encode(&data);
                    ui_state.pending_images.push((base64_data, mime.to_string()));
                    ui_state.status_message = Some(format!(
                        "Attached: {}",
                        path_buf.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }
        }
    }
}

/// Main UI system using egui
fn ui_system(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<UiState>,
    connection: Res<CoreConnection>,
    mut voice_state: ResMut<VoiceState>,
    mut image_cache: ResMut<ImageCache>,
    audio_player: Res<AudioPlayer>,
    mut audio_play_state: ResMut<AudioPlayState>,
) {
    let ctx = contexts.ctx_mut();

    // Collapsible side panel for conversations
    egui::SidePanel::left("conversations_panel")
        .resizable(true)
        .default_width(200.0)
        .show_animated(ctx, ui_state.side_panel_open, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Conversations");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("+").clicked() {
                        let _ = connection.cmd_tx.send(AppCommand::NewConversation);
                    }
                });
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                let current_id = ui_state.current_conversation_id.clone();
                let mut switch_to = None;
                let mut delete_id = None;

                for convo in &ui_state.conversations {
                    let is_current = current_id.as_ref() == Some(&convo.id);
                    let name = convo
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Chat ({})", convo.message_count));

                    ui.horizontal(|ui| {
                        let label = if is_current {
                            egui::RichText::new(&name).strong()
                        } else {
                            egui::RichText::new(&name)
                        };

                        if ui.selectable_label(is_current, label).clicked() && !is_current {
                            switch_to = Some(convo.id.clone());
                        }

                        // Delete button (only for non-current)
                        if !is_current {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("×").clicked() {
                                        delete_id = Some(convo.id.clone());
                                    }
                                },
                            );
                        }
                    });
                }

                if let Some(id) = switch_to {
                    let _ = connection.cmd_tx.send(AppCommand::SwitchConversation(id));
                }
                if let Some(id) = delete_id {
                    let _ = connection.cmd_tx.send(AppCommand::DeleteConversation(id));
                }
            });
        });

    // Top panel with title, model dropdown, and status
    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            // Toggle side panel button
            let panel_icon = if ui_state.side_panel_open {
                "◀"
            } else {
                "▶"
            };
            if ui.button(panel_icon).clicked() {
                ui_state.side_panel_open = !ui_state.side_panel_open;
                // Request conversations list when opening
                if ui_state.side_panel_open {
                    let _ = connection.cmd_tx.send(AppCommand::ListConversations);
                }
            }

            ui.heading("Noema");
            ui.separator();

            // Model dropdown
            egui::ComboBox::from_label("")
                .selected_text(format!("Model: {}", ui_state.model_name))
                .show_ui(ui, |ui| {
                    for (idx, (provider_id, provider_name)) in PROVIDERS.iter().enumerate() {
                        if ui
                            .selectable_label(idx == ui_state.selected_provider_idx, *provider_name)
                            .clicked()
                        {
                            ui_state.selected_provider_idx = idx;
                            let _ = connection.cmd_tx.send(AppCommand::SetModel {
                                provider: provider_id.to_string(),
                                model: None,
                            });
                        }
                    }
                });

            if ui_state.is_streaming {
                ui.spinner();
                ui.label("Thinking...");
            }

            // Voice status indicator (when enabled)
            if ui_state.voice_enabled {
                ui.separator();
                if ui_state.voice_transcribing {
                    ui.spinner();
                    ui.label("Transcribing...");
                } else if ui_state.voice_listening {
                    ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "🎤 Listening...");
                } else {
                    ui.label("🎤 Voice On");
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Clear").clicked() {
                    let _ = connection.cmd_tx.send(AppCommand::ClearHistory);
                }
            });
        });

        if let Some(ref status) = ui_state.status_message {
            ui.colored_label(egui::Color32::YELLOW, status);
        }
    });

    // Bottom panel with input and mic button
    let bottom_height = if ui_state.pending_images.is_empty() {
        56.0
    } else {
        90.0
    };
    egui::TopBottomPanel::bottom("input_panel")
        .exact_height(bottom_height)
        .show(ctx, |ui| {
            // Show pending images
            if !ui_state.pending_images.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("Attachments:");
                    let mut to_remove = Vec::new();
                    for (i, (_, mime)) in ui_state.pending_images.iter().enumerate() {
                        let label = format!("{} ×", mime.split('/').last().unwrap_or("image"));
                        if ui.button(&label).clicked() {
                            to_remove.push(i);
                        }
                    }
                    for i in to_remove.into_iter().rev() {
                        ui_state.pending_images.remove(i);
                    }
                });
                ui.add_space(4.0);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let available = ui.available_width();
                let button_width = 120.0; // Send + Mic buttons
                let input_width = (available - button_width).max(100.0);

                let input_response = ui.add_sized(
                    [input_width, 32.0],
                    egui::TextEdit::singleline(&mut ui_state.input_text)
                        .hint_text("Type a message... (or drop images)"),
                );

                let enter_pressed =
                    input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let cmd_enter =
                    ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);

                let send_clicked = ui.add_sized([60.0, 32.0], egui::Button::new("Send")).clicked();

                // Mic button with icon (moved to input bar)
                let mic_icon = if ui_state.voice_enabled { "🎤" } else { "🔇" };
                let mic_button = ui.add_sized([40.0, 32.0], egui::Button::new(mic_icon));
                if mic_button.clicked() {
                    if ui_state.voice_enabled {
                        voice_state.coordinator = None;
                        ui_state.voice_enabled = false;
                        ui_state.voice_listening = false;
                        ui_state.voice_transcribing = false;
                        ui_state.status_message = Some("Voice disabled".to_string());
                    } else {
                        match VoiceAgent::new(&voice_state.whisper_model_path) {
                            Ok(agent) => {
                                voice_state.coordinator = Some(VoiceCoordinator::new(agent));
                                ui_state.voice_enabled = true;
                                ui_state.status_message =
                                    Some("Voice enabled - speak to send".to_string());
                            }
                            Err(e) => {
                                ui_state.status_message =
                                    Some(format!("Voice init failed: {}", e));
                            }
                        }
                    }
                }
                mic_button.on_hover_text(if ui_state.voice_enabled {
                    "Disable voice input"
                } else {
                    "Enable voice input"
                });

                if (send_clicked || enter_pressed || cmd_enter)
                    && (!ui_state.input_text.trim().is_empty()
                        || !ui_state.pending_images.is_empty())
                    && !ui_state.is_streaming
                {
                    let text = std::mem::take(&mut ui_state.input_text);
                    let images = std::mem::take(&mut ui_state.pending_images);

                    // Build multimodal ChatPayload
                    let mut content_blocks = Vec::new();
                    if !text.trim().is_empty() {
                        content_blocks.push(ContentBlock::Text { text });
                    }
                    for (data, mime_type) in images {
                        content_blocks.push(ContentBlock::Image { data, mime_type });
                    }

                    let payload = ChatPayload::new(content_blocks);
                    let _ = connection.cmd_tx.send(AppCommand::SendMessage(payload));
                    ui_state.scroll_to_bottom = true;
                    ui_state.status_message = None;
                }

                if !ui_state.is_streaming {
                    input_response.request_focus();
                }
            });
        });

    // Central panel with messages
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .drag_to_scroll(true)
            .stick_to_bottom(ui_state.scroll_to_bottom)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                for msg in &ui_state.messages {
                    render_message(
                        ui,
                        msg,
                        &mut image_cache,
                        &audio_player,
                        &mut audio_play_state,
                    );
                }

                // Show streaming messages (multimodal)
                if ui_state.is_streaming && !ui_state.streaming_messages.is_empty() {
                    for msg in &ui_state.streaming_messages {
                        render_message(
                            ui,
                            msg,
                            &mut image_cache,
                            &audio_player,
                            &mut audio_play_state,
                        );
                    }
                }

                if ui_state.scroll_to_bottom {
                    ui_state.scroll_to_bottom = false;
                }
            });
    });
}

fn render_message(
    ui: &mut egui::Ui,
    msg: &DisplayMessage,
    image_cache: &mut ResMut<ImageCache>,
    audio_player: &Res<AudioPlayer>,
    audio_play_state: &mut ResMut<AudioPlayState>,
) {
    ui.add_space(8.0);

    let (role_label, role_color, bg_color) = match msg.role {
        MessageRole::User => (
            "[You]",
            egui::Color32::from_rgb(100, 180, 255),
            egui::Color32::from_gray(50),
        ),
        MessageRole::Assistant => (
            "[Assistant]",
            egui::Color32::from_rgb(100, 200, 100),
            egui::Color32::from_gray(40),
        ),
        MessageRole::System => (
            "[System]",
            egui::Color32::from_rgb(255, 200, 100),
            egui::Color32::from_gray(35),
        ),
    };

    ui.horizontal(|ui| {
        ui.colored_label(role_color, role_label);
    });

    egui::Frame::none()
        .fill(bg_color)
        .rounding(4.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            for content in &msg.content {
                render_content_block(ui, content, image_cache, audio_player, audio_play_state);
            }
        });
}

fn render_content_block(
    ui: &mut egui::Ui,
    content: &DisplayContent,
    image_cache: &mut ResMut<ImageCache>,
    _audio_player: &Res<AudioPlayer>,
    audio_play_state: &mut ResMut<AudioPlayState>,
) {
    match content {
        DisplayContent::Text(text) => {
            ui.label(text);
        }
        DisplayContent::Image { data, mime_type: _ } => {
            render_image(ui, data, image_cache);
        }
        DisplayContent::Audio { data, mime_type } => {
            render_audio_player(ui, data, mime_type, audio_play_state);
        }
        DisplayContent::ToolCall { name, id } => {
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(200, 200, 100),
                    format!("Tool: {} ({})", name, &id[..8.min(id.len())]),
                );
            });
        }
        DisplayContent::ToolResult { id, content } => {
            ui.vertical(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(150, 200, 150),
                    format!("Result ({})", &id[..8.min(id.len())]),
                );
                for item in content {
                    match item {
                        DisplayToolResultContent::Text(text) => {
                            ui.label(text);
                        }
                        DisplayToolResultContent::Image { data, mime_type: _ } => {
                            render_image(ui, data, image_cache);
                        }
                        DisplayToolResultContent::Audio { data, mime_type } => {
                            render_audio_player(ui, data, mime_type, audio_play_state);
                        }
                    }
                }
            });
        }
    }
}

fn render_image(ui: &mut egui::Ui, data: &str, image_cache: &mut ResMut<ImageCache>) {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    let hash = hasher.finish();

    // Check cache first
    if let Some(texture) = image_cache.textures.get(&hash) {
        let size = texture.size_vec2();
        let max_width = ui.available_width().min(400.0);
        let scale = if size.x > max_width {
            max_width / size.x
        } else {
            1.0
        };
        ui.image(egui::ImageSource::Texture(egui::load::SizedTexture::new(
            texture.id(),
            size * scale,
        )));
        return;
    }

    // Decode and cache
    match base64::engine::general_purpose::STANDARD.decode(data) {
        Ok(bytes) => match image::load_from_memory(&bytes) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let pixels = rgba.into_raw();

                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                let texture = ui.ctx().load_texture(
                    format!("img_{}", hash),
                    color_image,
                    egui::TextureOptions::default(),
                );

                let tex_size = texture.size_vec2();
                let max_width = ui.available_width().min(400.0);
                let scale = if tex_size.x > max_width {
                    max_width / tex_size.x
                } else {
                    1.0
                };

                ui.image(egui::ImageSource::Texture(egui::load::SizedTexture::new(
                    texture.id(),
                    tex_size * scale,
                )));

                image_cache.textures.insert(hash, texture);
            }
            Err(e) => {
                ui.colored_label(egui::Color32::RED, format!("Failed to decode image: {}", e));
            }
        },
        Err(e) => {
            ui.colored_label(egui::Color32::RED, format!("Invalid base64 image: {}", e));
        }
    }
}

fn render_audio_player(
    ui: &mut egui::Ui,
    data: &str,
    mime_type: &str,
    audio_play_state: &mut ResMut<AudioPlayState>,
) {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    let hash = hasher.finish();

    let is_playing = audio_play_state.playing.contains(&hash);

    ui.horizontal(|ui| {
        let button_text = if is_playing { "⏹" } else { "▶" };

        if ui.button(button_text).clicked() {
            if is_playing {
                audio_play_state.playing.remove(&hash);
            } else {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                    audio_play_state.playing.insert(hash);

                    // Spawn a thread to play audio (simplified - assumes raw PCM)
                    std::thread::spawn(move || {
                        if let Ok(pb) = AudioPlayback::new() {
                            let samples: Vec<f32> = bytes
                                .chunks_exact(2)
                                .map(|chunk| {
                                    let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                                    sample as f32 / i16::MAX as f32
                                })
                                .collect();
                            let _ = pb.play_samples(&samples);
                        }
                    });
                }
            }
        }

        ui.label(format!("Audio ({})", mime_type));
    });
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Starting Noema GUI");

    load_env_file();
    let provider_urls = ProviderUrls::from_env();

    let (cmd_tx, cmd_rx) = unbounded::<AppCommand>();
    let (event_tx, event_rx) = unbounded::<CoreEvent>();

    spawn_async_backend(cmd_rx, event_tx, provider_urls);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Noema".to_string(),
                resolution: (1000.0, 800.0).into(),
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        .insert_resource(CoreConnection { cmd_tx, event_rx })
        .insert_resource(UiState::default())
        .insert_resource(UiScale::default())
        .insert_resource(VoiceState {
            coordinator: None,
            whisper_model_path: get_whisper_model_path(),
        })
        .insert_resource(AudioPlayer::default())
        .insert_resource(ImageCache::default())
        .insert_resource(AudioPlayState::default())
        .add_systems(Startup, setup_egui)
        .add_systems(
            Update,
            (
                event_reader_system,
                voice_system,
                file_drop_system,
                ui_system,
            )
                .chain(),
        )
        .run();
}
