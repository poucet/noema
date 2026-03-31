//! Chat-related Tauri commands

use llm::Role;
use simply_core::storage::{EntityStore, EntityType, InputContent, StorageTypes, Stores, TurnStore};
use simply_core::storage::ids::{ConversationId, TurnId, SpanId};
use simply_core::storage::traits::ReferenceStore;
use simply_daemon::api::{
    CreateSessionOptions, DaemonEvent, SessionApi, SessionId, ModelApi, UserMessage,
    ToolFilter as DaemonToolFilter,
};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::broadcast;

use crate::logging::log_message;
use crate::state::AppState;
use crate::types::{
    AlternateInfo, ConversationInfo, DisplayMessage, DisplayInputContent,
    ErrorEvent, MessageCompleteEvent, StreamingMessageEvent,
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
                    DaemonEvent::AssistantContent(block) => {
                        state.set_processing(&conversation_id, true).await;
                        let content = vec![crate::types::DisplayContent::from(&block)];
                        let msg = DisplayMessage {
                            role: llm::Role::Assistant,
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
                        // Re-fetch all messages for the complete event
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

/// Enrich messages with alternate span information for each turn
async fn enrich_with_alternates<S: StorageTypes, T: Stores<S>>(
    messages: Vec<DisplayMessage>,
    stores: &T,
    conversation_id: &ConversationId,
) -> Vec<DisplayMessage> {
    let turn_ids: Vec<TurnId> = messages
        .iter()
        .filter_map(|m| m.turn_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut alternates_map: HashMap<TurnId, Vec<AlternateInfo>> = HashMap::new();

    for turn_id in turn_ids {
        if let Ok(spans) = stores.turn().get_spans(&turn_id).await {
            if spans.len() > 1 {
                let selected_span = stores.turn()
                    .get_selected_span(conversation_id, &turn_id)
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
                            model_display_name: span.model_id.clone(),
                            message_count: 0,
                            is_selected,
                        }
                    })
                    .collect();
                alternates_map.insert(turn_id, alternates);
            }
        }
    }

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

/// Get current messages in the conversation
#[tauri::command]
pub async fn get_messages(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Vec<DisplayMessage>, String> {
    let stores = state.get_stores()?;
    let daemon = state.get_daemon()?;
    let session_id = SessionId::new(conversation_id.as_str());

    let msgs: Vec<DisplayMessage> = daemon
        .get_messages(&session_id)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))?
        .iter()
        .map(DisplayMessage::from)
        .collect();

    let msgs = enrich_with_alternates(msgs, stores, &conversation_id).await;
    Ok(msgs)
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

    let daemon_tool_filter = tool_config.map(|tc| DaemonToolFilter {
        server_ids: tc.server_ids,
        tool_names: tc.tool_names,
    });

    daemon
        .send_message(&session_id, UserMessage {
            content: input_content,
            tool_filter: daemon_tool_filter,
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

    // Also update daemon default
    daemon
        .set_default_model(&full_model_id)
        .await
        .map_err(|e| format!("Failed to set default model: {}", e))?;

    let display_name = model_id
        .split('/')
        .last()
        .unwrap_or(&model_id)
        .to_string();

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
pub async fn list_models(state: State<'_, Arc<AppState>>) -> Result<Vec<ModelInfo>, String> {
    use llm::ModelCapability;

    let daemon = state.get_daemon()?;
    let all = daemon.list_models().await.map_err(|e| format!("Failed to list models: {}", e))?;

    let mut result = Vec::new();
    for m in all {
        if !m.definition.has_capability(&ModelCapability::Text) {
            continue;
        }
        let capabilities: Vec<String> = m.definition.capabilities.iter().map(|c| format!("{:?}", c)).collect();
        // Extract provider from model ID (format: "provider/model")
        let provider = m.id.provider.clone();
        result.push(ModelInfo {
            id: m.definition.id.clone(),
            display_name: m.definition.name().to_string(),
            provider,
            capabilities,
            context_window: m.definition.context_window,
        });
    }

    Ok(result)
}

/// List all conversations for the current user
#[tauri::command]
pub async fn list_conversations(state: State<'_, Arc<AppState>>) -> Result<Vec<ConversationInfo>, String> {
    let stores = state.get_stores()?;
    let user_id = state.user_id.lock().await.clone();

    let entities = stores
        .entity()
        .list_entities(&user_id, Some(&EntityType::conversation()))
        .await
        .map_err(|e| format!("Failed to list conversations: {}", e))?;

    let mut result = Vec::with_capacity(entities.len());
    for entity in entities {
        let turn_count = stores
            .turn()
            .get_turn_count(&entity.id)
            .await
            .unwrap_or(0);
        result.push(ConversationInfo::from_entity(&entity, turn_count));
    }

    Ok(result)
}

/// Load a conversation (creating a daemon session for it)
#[tauri::command]
pub async fn load_conversation(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Vec<DisplayMessage>, String> {
    let stores = state.get_stores()?;
    let daemon = state.get_daemon()?;
    let session_id = SessionId::new(conversation_id.as_str());

    // Resume session (loads from storage if not already open, returns existing if open)
    let (_info, rx) = daemon
        .resume_session(&session_id)
        .await
        .map_err(|e| format!("Failed to load conversation: {}", e))?;

    // Forward daemon events to Tauri UI
    spawn_event_forwarder(app, state.inner().clone(), conversation_id.clone(), rx);

    let messages: Vec<DisplayMessage> = daemon
        .get_messages(&session_id)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))?
        .iter()
        .map(DisplayMessage::from)
        .collect();

    let messages = enrich_with_alternates(messages, stores, &conversation_id).await;
    Ok(messages)
}

/// Create a new conversation
#[tauri::command]
pub async fn new_conversation(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    name: Option<String>,
) -> Result<String, String> {
    let coordinator = state.get_coordinator()?;
    let daemon = state.get_daemon()?;
    let user_id = state.user_id.lock().await.clone();

    let conversation_name = name.unwrap_or_else(|| {
        let now = chrono::Utc::now();
        format!("Chat {}", now.format("%b %d, %H:%M"))
    });

    // Create the entity in storage
    let conv_id = coordinator
        .create_conversation(&user_id, Some(&conversation_name))
        .await
        .map_err(|e| format!("Failed to create conversation: {}", e))?;

    // Create a daemon session and start event forwarding
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
    let session_id = SessionId::new(conversation_id.as_str());

    // Close daemon session (ignore error if not loaded)
    let _ = daemon.close_session(&session_id).await;

    let stores = state.get_stores()?;
    stores
        .entity()
        .delete_entity(&conversation_id)
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

    let mut entity = stores
        .entity()
        .get_entity(&conversation_id)
        .await
        .map_err(|e| format!("Failed to get conversation: {}", e))?
        .ok_or_else(|| "Conversation not found".to_string())?;

    entity.name = if name.trim().is_empty() { None } else { Some(name) };

    stores
        .entity()
        .update_entity(&conversation_id, &entity)
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
    let entity = stores
        .entity()
        .get_entity(&conversation_id)
        .await
        .map_err(|e| format!("Failed to get conversation: {}", e))?
        .ok_or_else(|| "Conversation not found".to_string())?;
    Ok(entity.is_private)
}

/// Set whether a conversation is marked as private
#[tauri::command]
pub async fn set_conversation_private(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    is_private: bool,
) -> Result<(), String> {
    let stores = state.get_stores()?;

    let mut entity = stores
        .entity()
        .get_entity(&conversation_id)
        .await
        .map_err(|e| format!("Failed to get conversation: {}", e))?
        .ok_or_else(|| "Conversation not found".to_string())?;

    entity.is_private = is_private;

    stores
        .entity()
        .update_entity(&conversation_id, &entity)
        .await
        .map_err(|e| format!("Failed to set conversation privacy: {}", e))
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

// ============================================================================
// Turn/Span Commands (direct store access — not yet in daemon API)
// ============================================================================

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
    let coordinator = state.get_coordinator()?;

    let messages = stores.turn()
        .get_messages(&span_id)
        .await
        .map_err(|e| format!("Failed to get span messages: {}", e))?;

    let mut result = Vec::with_capacity(messages.len());
    for m in messages {
        let resolved = coordinator
            .resolve_stored_content(&m.content)
            .await
            .map_err(|e| format!("Failed to resolve content: {}", e))?;

        let content = resolved.iter().map(crate::types::DisplayContent::from).collect();

        result.push(DisplayMessage {
            role: Role::from(m.message.role),
            content,
            turn_id: None,
            span_id: Some(span_id.clone()),
            alternates: None,
        });
    }

    Ok(result)
}

/// Select a specific span at a turn
#[tauri::command]
pub async fn select_span(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    turn_id: TurnId,
    span_id: SpanId,
) -> Result<(), String> {
    let stores = state.get_stores()?;
    let daemon = state.get_daemon()?;

    stores.turn()
        .select_span(&conversation_id, &turn_id, &span_id)
        .await
        .map_err(|e| format!("Failed to select span: {}", e))?;

    // Reload the session to reflect the new selection
    let session_id = SessionId::new(conversation_id.as_str());
    daemon
        .reload(&session_id)
        .await
        .map_err(|e| format!("Failed to reload messages: {}", e))
}

// ============================================================================
// Fork Commands (direct store access)
// ============================================================================

use crate::types::ForkInfoResponse;

/// Fork a conversation at a specific turn
#[tauri::command]
pub async fn fork_conversation(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
    at_turn_id: TurnId,
) -> Result<String, String> {
    let coordinator = state.get_coordinator()?;

    let new_conversation_id = coordinator
        .fork_conversation(&conversation_id, &at_turn_id, None)
        .await
        .map_err(|e| format!("Failed to fork conversation: {}", e))?;

    Ok(new_conversation_id.as_str().to_string())
}

/// List all forks of a conversation
#[tauri::command]
pub async fn list_conversation_views(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Vec<ForkInfoResponse>, String> {
    let stores = state.get_stores()?;
    let coordinator = state.get_coordinator()?;

    let _entity = stores.entity()
        .get_entity(&conversation_id)
        .await
        .map_err(|e| format!("Failed to get conversation: {}", e))?
        .ok_or_else(|| format!("Conversation not found: {}", conversation_id))?;

    let forks = coordinator
        .get_forked_conversations(&conversation_id)
        .await
        .map_err(|e| format!("Failed to list forks: {}", e))?;

    let mut result = Vec::with_capacity(forks.len());
    for (fork_id, at_turn_id) in forks {
        let turn_count = stores.turn().get_turn_count(&fork_id).await.unwrap_or(0);
        let fork_entity = stores.entity().get_entity(&fork_id).await.ok().flatten();
        let created_at = fork_entity.map(|e| e.created_at).unwrap_or(0);
        result.push(ForkInfoResponse {
            conversation_id: fork_id,
            forked_at_turn_id: at_turn_id,
            turn_count,
            created_at,
        });
    }

    Ok(result)
}

/// List conversations forked from a given conversation.
#[tauri::command]
pub async fn list_forked_conversations(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Vec<ForkedConversationInfo>, String> {
    let coordinator = state.get_coordinator()?;

    let forks = coordinator
        .get_forked_conversations(&conversation_id)
        .await
        .map_err(|e| format!("Failed to list forked conversations: {}", e))?;

    Ok(forks
        .into_iter()
        .map(|(fork_id, at_turn_id)| ForkedConversationInfo {
            conversation_id: fork_id.as_str().to_string(),
            at_turn_id: at_turn_id.as_str().to_string(),
        })
        .collect())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkedConversationInfo {
    pub conversation_id: String,
    pub at_turn_id: String,
}

// ============================================================================
// Subconversation Commands (direct store access)
// ============================================================================

/// Spawn a subconversation from a parent conversation.
#[tauri::command]
pub async fn spawn_subconversation(
    state: State<'_, Arc<AppState>>,
    parent_conversation_id: ConversationId,
    at_turn_id: TurnId,
    at_span_id: Option<SpanId>,
    name: Option<String>,
) -> Result<String, String> {
    let coordinator = state.get_coordinator()?;
    let user_id = state.user_id.lock().await.clone();

    let sub_id = coordinator
        .spawn_subconversation(
            &parent_conversation_id,
            &user_id,
            &at_turn_id,
            at_span_id.as_ref(),
            name.as_deref(),
        )
        .await
        .map_err(|e| format!("Failed to spawn subconversation: {}", e))?;

    Ok(sub_id.as_str().to_string())
}

/// Get the parent conversation for a subconversation.
#[tauri::command]
pub async fn get_parent_conversation(
    state: State<'_, Arc<AppState>>,
    conversation_id: ConversationId,
) -> Result<Option<ParentConversationInfo>, String> {
    let coordinator = state.get_coordinator()?;

    match coordinator.get_parent_conversation(&conversation_id).await {
        Ok(Some((parent_id, at_turn_id, at_span_id))) => {
            Ok(Some(ParentConversationInfo {
                parent_conversation_id: parent_id.as_str().to_string(),
                at_turn_id: at_turn_id.as_str().to_string(),
                at_span_id: at_span_id.map(|s| s.as_str().to_string()),
            }))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(format!("Failed to get parent conversation: {}", e)),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentConversationInfo {
    pub parent_conversation_id: String,
    pub at_turn_id: String,
    pub at_span_id: Option<String>,
}

/// List all subconversations spawned from a parent conversation.
#[tauri::command]
pub async fn list_subconversations(
    state: State<'_, Arc<AppState>>,
    parent_conversation_id: ConversationId,
) -> Result<Vec<SubconversationInfo>, String> {
    let coordinator = state.get_coordinator()?;

    let subs = coordinator
        .list_subconversations(&parent_conversation_id)
        .await
        .map_err(|e| format!("Failed to list subconversations: {}", e))?;

    Ok(subs
        .into_iter()
        .map(|(sub_id, at_turn_id, at_span_id)| SubconversationInfo {
            conversation_id: sub_id.as_str().to_string(),
            at_turn_id: at_turn_id.as_str().to_string(),
            at_span_id: at_span_id.map(|s| s.as_str().to_string()),
        })
        .collect())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubconversationInfo {
    pub conversation_id: String,
    pub at_turn_id: String,
    pub at_span_id: Option<String>,
}

/// Get the final result from a subconversation.
#[tauri::command]
pub async fn get_subconversation_result(
    state: State<'_, Arc<AppState>>,
    subconversation_id: ConversationId,
) -> Result<Option<String>, String> {
    let coordinator = state.get_coordinator()?;

    coordinator
        .get_subconversation_result(&subconversation_id)
        .await
        .map_err(|e| format!("Failed to get subconversation result: {}", e))
}

/// Link a subconversation's result back to the parent conversation.
#[tauri::command]
pub async fn link_subconversation_result(
    state: State<'_, Arc<AppState>>,
    subconversation_id: ConversationId,
    parent_conversation_id: ConversationId,
    parent_span_id: SpanId,
    parent_turn_id: TurnId,
    tool_call_id: String,
    tool_name: String,
) -> Result<(), String> {
    let coordinator = state.get_coordinator()?;
    let daemon = state.get_daemon()?;

    coordinator
        .link_subconversation_result(
            &subconversation_id,
            &parent_span_id,
            &parent_turn_id,
            &tool_call_id,
            &tool_name,
        )
        .await
        .map_err(|e| format!("Failed to link subconversation result: {}", e))?;

    // Reload the parent session
    let session_id = SessionId::new(parent_conversation_id.as_str());
    let _ = daemon.reload(&session_id).await;

    Ok(())
}

// ============================================================================
// Cross-Reference Commands (direct store access)
// ============================================================================

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceInfo {
    pub id: String,
    pub from_entity_id: String,
    pub to_entity_id: String,
    pub relation_type: Option<String>,
    pub context: Option<String>,
    pub created_at: i64,
}

#[tauri::command]
pub async fn create_reference(
    state: State<'_, Arc<AppState>>,
    from_entity_id: String,
    to_entity_id: String,
    relation_type: Option<String>,
    context: Option<String>,
) -> Result<String, String> {
    use simply_core::storage::{ids::EntityId, types::RelationType};

    let stores = state.get_stores()?;

    let from_id = EntityId::from_string(from_entity_id);
    let to_id = EntityId::from_string(to_entity_id);
    let rel_type = relation_type.map(RelationType::new);

    let ref_id = stores
        .reference()
        .create_reference(&from_id, &to_id, rel_type.as_ref(), context.as_deref())
        .await
        .map_err(|e| format!("Failed to create reference: {}", e))?;

    Ok(ref_id.as_str().to_string())
}

#[tauri::command]
pub async fn delete_reference(
    state: State<'_, Arc<AppState>>,
    reference_id: String,
) -> Result<bool, String> {
    use simply_core::storage::ids::ReferenceId;

    let stores = state.get_stores()?;
    let ref_id = ReferenceId::from_string(reference_id);

    stores
        .reference()
        .delete_reference(&ref_id)
        .await
        .map_err(|e| format!("Failed to delete reference: {}", e))
}

#[tauri::command]
pub async fn get_entity_references(
    state: State<'_, Arc<AppState>>,
    entity_id: String,
) -> Result<Vec<ReferenceInfo>, String> {
    use simply_core::storage::ids::EntityId;

    let stores = state.get_stores()?;
    let id = EntityId::from_string(entity_id);

    let refs = stores
        .reference()
        .get_outgoing(&id)
        .await
        .map_err(|e| format!("Failed to get references: {}", e))?;

    Ok(refs
        .into_iter()
        .map(|r| ReferenceInfo {
            id: r.id.as_str().to_string(),
            from_entity_id: r.from_entity_id.as_str().to_string(),
            to_entity_id: r.to_entity_id.as_str().to_string(),
            relation_type: r.relation_type.clone().map(|t| t.as_str().to_string()),
            context: r.context.clone(),
            created_at: r.created_at,
        })
        .collect())
}

#[tauri::command]
pub async fn get_entity_backlinks(
    state: State<'_, Arc<AppState>>,
    entity_id: String,
) -> Result<Vec<ReferenceInfo>, String> {
    use simply_core::storage::ids::EntityId;

    let stores = state.get_stores()?;
    let id = EntityId::from_string(entity_id);

    let refs = stores
        .reference()
        .get_backlinks(&id)
        .await
        .map_err(|e| format!("Failed to get backlinks: {}", e))?;

    Ok(refs
        .into_iter()
        .map(|r| ReferenceInfo {
            id: r.id.as_str().to_string(),
            from_entity_id: r.from_entity_id.as_str().to_string(),
            to_entity_id: r.to_entity_id.as_str().to_string(),
            relation_type: r.relation_type.clone().map(|t| t.as_str().to_string()),
            context: r.context.clone(),
            created_at: r.created_at,
        })
        .collect())
}
