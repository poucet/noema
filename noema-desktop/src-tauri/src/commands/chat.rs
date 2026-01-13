//! Chat-related Tauri commands

use llm::{Role, create_model, list_all_models};
use noema_core::{ConversationManager, ManagerEvent, ToolConfig as CoreToolConfig};
use noema_core::storage::{ConversationStore, DocumentResolver, MessageRole, InputContent, Session, Stores, TurnStore};
use noema_core::storage::ids::{ConversationId, TurnId, SpanId, ViewId};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::logging::log_message;
use crate::state::AppState;
use crate::types::{
    AlternateInfo, ConversationInfo, DisplayMessage, ErrorEvent, TruncatedEvent, DisplayInputContent,
    MessageCompleteEvent, ModelChangedEvent, ModelInfo, StreamingMessageEvent, ToolConfig,
    UserMessageEvent,
};

/// Enrich messages with alternate span information for each turn
async fn enrich_with_alternates<S: Stores>(
    messages: Vec<DisplayMessage>,
    stores: &S,
    view_id: &ViewId,
) -> Vec<DisplayMessage> {
    // Collect unique turn IDs from messages
    let turn_ids: Vec<TurnId> = messages
        .iter()
        .filter_map(|m| m.turn_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Build a map of turn_id -> Vec<AlternateInfo>
    let mut alternates_map: HashMap<TurnId, Vec<AlternateInfo>> = HashMap::new();

    for turn_id in turn_ids {
        // Get all spans for this turn
        if let Ok(spans) = stores.turn().get_spans(&turn_id).await {
            if spans.len() > 1 {
                // Get the selected span for this view
                let selected_span = stores.turn()
                    .get_selected_span(view_id, &turn_id)
                    .await
                    .ok()
                    .flatten();

                let alternates: Vec<AlternateInfo> = spans
                    .into_iter()
                    .map(|span| {
                        let is_selected = selected_span.as_ref() == Some(&span.id);
                        AlternateInfo {
                            span_id: span.id.clone(),
                            model_id: span.model_id.clone(),
                            model_display_name: span.model_id.clone(), // Could be enhanced with display name lookup
                            message_count: 0, // Not currently tracked
                            is_selected,
                        }
                    })
                    .collect();
                alternates_map.insert(turn_id, alternates);
            }
        }
    }

    // Enrich messages with alternates
    messages
        .into_iter()
        .map(|mut msg| {
            if let Some(turn_id) = &msg.turn_id {
                if let Some(alternates) = alternates_map.get(turn_id) {
                    if alternates.len() > 1 {
                        msg.alternates = Some(alternates.clone());
                    }
                }
            }
            msg
        })
        .collect()
}

/// Get current messages in the conversation (committed + pending)
#[tauri::command]
pub async fn get_messages(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Vec<DisplayMessage>, String> {
    let managers = state.managers.lock().await;
    let manager = managers.get(&conversation_id).ok_or("Conversation not loaded")?;

    let msgs: Vec<DisplayMessage> = manager
        .all_messages()
        .await
        .iter()
        .map(DisplayMessage::from)
        .collect();

    Ok(msgs)
}

/// Send a message with structured content blocks.
/// Content blocks preserve the exact inline position of text, document references, and attachments.
///
/// # Arguments
/// * `conversation_id` - The conversation to send the message to
/// * `content` - The message content blocks (text, document refs, images, audio)
/// * `tool_config` - Optional configuration for which tools to enable. If None, uses default (all tools enabled).
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

    // Convert Tauri DisplayInputContent to core InputContent, filtering empty text
    let input_content: Vec<InputContent> = content
        .into_iter()
        .filter(|block| !matches!(block, DisplayInputContent::Text { text } if text.is_empty()))
        .map(InputContent::from)
        .collect();

    if input_content.is_empty() {
        return Err("Message must have text, documents, or attachments".to_string());
    }

    // Convert ToolConfig from Tauri types to core types
    let core_tool_config = match tool_config {
        Some(tc) => CoreToolConfig {
            enabled: tc.enabled,
            server_ids: tc.server_ids,
            tool_names: tc.tool_names,
        },
        None => CoreToolConfig::all_enabled(),
    };

    // Send message via manager - it handles storage, agent execution, and commit
    let managers = state.managers.lock().await;
    let manager = managers.get(&conversation_id).ok_or("Conversation not loaded")?;
    manager.send_message(input_content, core_tool_config);

    Ok(())
}

/// Start the shared event receiver loop - runs continuously from app init
/// Receives events from the shared channel that all managers send to
pub async fn start_event_receiver_loop(app: AppHandle, state: Arc<AppState>) {
    // Take the receiver - can only be done once
    let mut event_rx = match state.take_event_receiver().await {
        Some(rx) => rx,
        None => {
            log_message("Event receiver already taken - event loop not started");
            return;
        }
    };

    tokio::spawn(async move {
        while let Some((conversation_id, event)) = event_rx.recv().await {
            match event {
                ManagerEvent::UserMessageAdded(msg) => {
                    let _ = app.emit("user_message", UserMessageEvent {
                        conversation_id: conversation_id.clone(),
                        message: DisplayMessage::from(&msg),
                    });
                }
                ManagerEvent::StreamingMessage(msg) => {
                    state.set_processing(&conversation_id, true).await;
                    let _ = app.emit("streaming_message", StreamingMessageEvent {
                        conversation_id: conversation_id.clone(),
                        message: DisplayMessage::from(&msg),
                    });
                }
                ManagerEvent::Complete(resolved_messages) => {
                    let messages: Vec<DisplayMessage> = resolved_messages
                        .iter()
                        .map(DisplayMessage::from)
                        .collect();
                    let _ = app.emit("message_complete", MessageCompleteEvent {
                        conversation_id: conversation_id.clone(),
                        messages,
                    });
                    state.set_processing(&conversation_id, false).await;
                }
                ManagerEvent::Error(err) => {
                    log_message(&format!("MANAGER ERROR [{}]: {}", conversation_id.as_str(), err));
                    let _ = app.emit("error", ErrorEvent {
                        conversation_id: conversation_id.clone(),
                        error: err,
                    });
                    state.set_processing(&conversation_id, false).await;
                }
                ManagerEvent::ModelChanged(name) => {
                    let _ = app.emit("model_changed", ModelChangedEvent {
                        conversation_id: conversation_id.clone(),
                        model: name,
                    });
                }
                ManagerEvent::Truncated(turn_id) => {
                    let _ = app.emit("truncated", TruncatedEvent {
                        conversation_id: conversation_id.clone(),
                        turn_id,
                    });
                }
            }
        }
    });
}

/// Clear conversation history
#[tauri::command]
pub async fn clear_history(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<(), String> {
    let managers = state.managers.lock().await;
    let manager = managers.get(&conversation_id).ok_or("Conversation not loaded")?;
    manager.clear_history();
    Ok(())
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

    let new_model = create_model(&full_model_id)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    let display_name = model_id
        .split('/')
        .last()
        .unwrap_or(&model_id)
        .to_string();

    {
        let mut managers = state.managers.lock().await;
        let manager = managers.get_mut(&conversation_id).ok_or("Conversation not loaded")?;
        manager.set_model(new_model);
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
    let stores = state.get_stores()?;
    let user_id = state.user_id.lock().await.clone();

    let convos = stores
        .conversation()
        .list_conversations(&user_id)
        .await
        .map_err(|e| format!("Failed to list conversations: {}", e))?;

    let mut result = Vec::with_capacity(convos.len());
    for conv in convos {
        let view = stores
            .turn()
            .get_view(&conv.main_view_id)
            .await
            .map_err(|e| format!("Failed to get view: {}", e))?
            .ok_or_else(|| format!("View not found: {}", conv.main_view_id))?;
        result.push(ConversationInfo::from_parts(&conv, &view));
    }

    Ok(result)
}

/// Load a conversation (creating a manager for it if not already loaded)
#[tauri::command]
pub async fn load_conversation(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Vec<DisplayMessage>, String> {
    let stores = state.get_stores()?;
    let coordinator = state.get_coordinator()?;

    // Check if already loaded
    {
        let managers = state.managers.lock().await;
        if let Some(manager) = managers.get(&conversation_id) {
            let view_id = manager.view_id().await;
            let messages: Vec<DisplayMessage> = manager
                .all_messages()
                .await
                .iter()
                .map(DisplayMessage::from)
                .collect();
            // Enrich with alternates
            let messages = enrich_with_alternates(messages, stores.as_ref(), &view_id).await;
            return Ok(messages);
        }
    }

    // Not loaded, create manager
    let session = Session::open(coordinator.clone(), conversation_id.clone())
        .await
        .map_err(|e| format!("Failed to open conversation: {}", e))?;

    let view_id = session.view_id().clone();
    let messages: Vec<DisplayMessage> = session
        .messages_for_display()
        .iter()
        .map(DisplayMessage::from)
        .collect();

    let model_id_str = state.model_id.lock().await.clone();
    let mcp_registry = state.get_mcp_registry()?;

    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    let document_resolver: Arc<dyn DocumentResolver> = stores.document();
    let event_tx = state.event_sender();
    let manager = ConversationManager::new(session, coordinator, model, mcp_registry, document_resolver, event_tx);
    state.managers.lock().await.insert(conversation_id, manager);

    // Enrich with alternates
    let messages = enrich_with_alternates(messages, stores.as_ref(), &view_id).await;
    Ok(messages)
}

/// Create a new conversation and load its manager
#[tauri::command]
pub async fn new_conversation(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let stores = state.get_stores()?;
    let coordinator = state.get_coordinator()?;
    let user_id = state.user_id.lock().await.clone();

    let conv_id = coordinator
        .create_conversation_with_view(&user_id, None)
        .await
        .map_err(|e| format!("Failed to create conversation: {}", e))?;

    let session = Session::open(coordinator.clone(), conv_id.clone())
        .await
        .map_err(|e| format!("Failed to open new conversation: {}", e))?;

    let model_id_str = state.model_id.lock().await.clone();
    let mcp_registry = state.get_mcp_registry()?;

    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    let document_resolver: Arc<dyn DocumentResolver> = stores.document();
    let event_tx = state.event_sender();
    let manager = ConversationManager::new(session, coordinator, model, mcp_registry, document_resolver, event_tx);
    state.managers.lock().await.insert(conv_id.clone(), manager);

    Ok(conv_id.as_str().to_string())
}

/// Delete a conversation
#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<(), String> {
    state.managers.lock().await.remove(&conversation_id);

    let stores = state.get_stores()?;
    stores
        .conversation()
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
    let stores = state.get_stores()?;

    let name_opt = if name.trim().is_empty() { None } else { Some(name.as_str()) };

    stores
        .conversation()
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
    let stores = state.get_stores()?;
    stores
        .conversation()
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
    let stores = state.get_stores()?;
    stores
        .conversation()
        .set_conversation_private(&conversation_id, is_private)
        .await
        .map_err(|e| format!("Failed to set conversation privacy: {}", e))
}

/// Get current model name
#[tauri::command]
pub async fn get_model_name(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    Ok(state.model_name.lock().await.clone())
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

// ============================================================================
// Turn/Span/View Commands
// ============================================================================

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
#[tauri::command]
pub async fn get_turn_alternates(
    state: State<'_, Arc<AppState>>,
    turn_id: TurnId,
) -> Result<Vec<SpanInfoResponse>, String> {
    let stores = state.get_stores()?;

    let spans = stores.turn()
        .get_spans(&turn_id)
        .await
        .map_err(|e| format!("Failed to get spans: {}", e))?;

    Ok(spans
        .into_iter()
        .map(|s| SpanInfoResponse {
            id: s.id.as_str().to_string(),
            model_id: s.model_id.clone(),
            message_count: s.message_count as usize,
            is_selected: false,
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
    let stores = state.get_stores()?;

    let messages = stores.turn()
        .get_messages(&span_id)
        .await
        .map_err(|e| format!("Failed to get span messages: {}", e))?;

    Ok(messages
        .into_iter()
        .map(|m| DisplayMessage {
            role: match m.message.role {
                MessageRole::User => Role::User,
                MessageRole::Assistant => Role::Assistant,
                MessageRole::System => Role::System,
                MessageRole::Tool => Role::Assistant,
            },
            content: vec![],
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
    let stores = state.get_stores()?;

    let conv = stores.conversation()
        .get_conversation(&conversation_id)
        .await
        .map_err(|e| format!("Failed to get conversation: {}", e))?
        .ok_or_else(|| format!("Conversation not found: {}", conversation_id))?;

    let views = stores.turn()
        .list_related_views(&conv.main_view_id)
        .await
        .map_err(|e| format!("Failed to list views: {}", e))?;

    Ok(views.into_iter().map(ThreadInfoResponse::from).collect())
}

/// Get the current view ID for a conversation
#[tauri::command]
pub async fn get_current_view_id(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Option<String>, String> {
    let managers = state.managers.lock().await;
    let manager = managers.get(&conversation_id).ok_or("Conversation not loaded")?;
    Ok(Some(manager.view_id().await.to_string()))
}

/// Regenerate response at a specific turn
#[tauri::command]
pub async fn regenerate_response(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    turn_id: TurnId,
    tool_config: Option<ToolConfig>,
) -> Result<(), String> {
    let core_tool_config = match tool_config {
        Some(tc) => CoreToolConfig {
            enabled: tc.enabled,
            server_ids: tc.server_ids,
            tool_names: tc.tool_names,
        },
        None => CoreToolConfig::all_enabled(),
    };

    let managers = state.managers.lock().await;
    let manager = managers.get(&conversation_id).ok_or("Conversation not loaded")?;
    manager.regenerate(turn_id, core_tool_config);

    Ok(())
}

/// Fork a conversation at a specific turn
#[tauri::command]
pub async fn fork_conversation(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    at_turn_id: TurnId,
) -> Result<ThreadInfoResponse, String> {
    let current_view_id = {
        let managers = state.managers.lock().await;
        let manager = managers.get(&conversation_id).ok_or("Conversation not loaded")?;
        manager.view_id().await
    };

    let stores = state.get_stores()?;

    let new_view = stores.turn()
        .fork_view(&current_view_id, &at_turn_id)
        .await
        .map_err(|e| format!("Failed to fork conversation: {}", e))?;

    Ok(ThreadInfoResponse::from(new_view))
}

/// Switch to a different view in a conversation
#[tauri::command]
pub async fn switch_view(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    view_id: ViewId,
) -> Result<Vec<DisplayMessage>, String> {
    let stores = state.get_stores()?;
    let coordinator = state.get_coordinator()?;

    let session = Session::open_view(coordinator.clone(), conversation_id.clone(), view_id)
        .await
        .map_err(|e| format!("Failed to open view: {}", e))?;

    let messages: Vec<DisplayMessage> = session
        .messages_for_display()
        .iter()
        .map(DisplayMessage::from)
        .collect();

    let model_id_str = state.model_id.lock().await.clone();
    let mcp_registry = state.get_mcp_registry()?;
    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    let document_resolver: Arc<dyn DocumentResolver> = stores.document();
    let event_tx = state.event_sender();
    let manager = ConversationManager::new(session, coordinator, model, mcp_registry, document_resolver, event_tx);
    state.managers.lock().await.insert(conversation_id, manager);

    Ok(messages)
}

/// Select a specific span at a turn
#[tauri::command]
pub async fn select_span(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    turn_id: TurnId,
    span_id: SpanId,
) -> Result<(), String> {
    let current_view_id = {
        let managers = state.managers.lock().await;
        let manager = managers.get(&conversation_id).ok_or("Conversation not loaded")?;
        manager.view_id().await
    };

    let stores = state.get_stores()?;

    stores.turn()
        .select_span(&current_view_id, &turn_id, &span_id)
        .await
        .map_err(|e| format!("Failed to select span: {}", e))?;

    // Clear the manager's cache so next get_messages returns updated path
    let managers = state.managers.lock().await;
    if let Some(manager) = managers.get(&conversation_id) {
        manager.clear_cache().await;
    }

    Ok(())
}