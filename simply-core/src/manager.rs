//! ConversationManager - orchestrates storage, session, and agent execution
//!
//! This is the main API for managing a conversation. It coordinates:
//! - Storage operations (storing user input, committing messages)
//! - Session state (pending messages, cache)
//! - Agent execution in a background task
//! - Event streaming to UI

use anyhow::Result;
use llm::{ChatMessage, ChatModel, ChatPayload};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::agents::{ExecutionContext, ToolEnricher};
use crate::context::ConversationContext;
use crate::storage::content::InputContent;
use crate::storage::coordinator::StorageCoordinator;
use crate::storage::ids::{ConversationId, SpanId, TurnId, UserId};
use crate::storage::session::{ResolvedMessage, Session};
use crate::storage::traits::StorageTypes;
use crate::storage::types::OriginKind;
use crate::storage::DocumentResolver;
use crate::{Agent, McpAgent, McpRegistry, McpToolRegistry};

/// Create an enricher that injects execution context for noema-core tools.
///
/// This keeps the "noema-core" knowledge in application code rather than library code.
fn create_noema_core_enricher() -> ToolEnricher {
    Arc::new(|tool_name, args, context| {
        // Inject context for noema-core tools (spawn_agent needs it)
        if tool_name == "spawn_agent" {
            match args {
                serde_json::Value::Object(map) => serde_json::Value::Object(context.inject_into(map)),
                other => other,
            }
        } else {
            args
        }
    })
}

/// Type alias for the shared event sender - sends (ConversationId, ManagerEvent) tuples
pub type SharedEventSender = mpsc::UnboundedSender<(ConversationId, ManagerEvent)>;

/// Configuration for which tools to enable during a request.
#[derive(Debug, Clone, Default)]
pub struct ToolConfig {
    pub enabled: bool,
    pub server_ids: Option<Vec<String>>,
    pub tool_names: Option<Vec<String>>,
}

impl ToolConfig {
    pub fn all_enabled() -> Self {
        Self { enabled: true, server_ids: None, tool_names: None }
    }

    pub fn disabled() -> Self {
        Self { enabled: false, server_ids: None, tool_names: None }
    }
}

/// How to commit messages after LLM execution.
#[derive(Debug, Clone, Default)]
pub enum CommitMode {
    /// Create new turns as needed based on message roles (normal flow)
    #[default]
    NewTurns,
    /// Add a new span at an existing turn (regeneration flow)
    AtTurn(TurnId),
}

// ============================================================================
// Commands and Events
// ============================================================================

/// Commands sent to the background task
pub enum ManagerCommand {
    /// Send user input, run agent, commit
    SendMessage {
        content: Vec<InputContent>,
        tool_config: ToolConfig,
    },
    /// Run agent on current pending (used after truncate for regeneration)
    RunAgent {
        tool_config: ToolConfig,
        commit_mode: CommitMode,
    },
    /// Truncate context to before a specific turn (None = clear all)
    Truncate(Option<TurnId>),
    /// Change the model (model_id should be in provider/model format)
    SetModel {
        model: Arc<dyn ChatModel + Send + Sync>,
        model_id: String,
    },
}

/// Events emitted from the background task
#[derive(Debug, Clone)]
pub enum ManagerEvent {
    /// User message was added (for immediate UI feedback)
    UserMessageAdded(ChatMessage),
    /// Streaming message from agent
    StreamingMessage(ChatMessage),
    /// Agent execution and commit completed - includes all committed messages with turn_ids
    Complete(Vec<ResolvedMessage>),
    /// Error occurred
    Error(String),
    /// Model was changed
    ModelChanged(String),
    /// Context was truncated
    Truncated(Option<TurnId>),
}

// ============================================================================
// ConversationManager
// ============================================================================

/// Manages a single conversation's lifecycle
///
/// Owns the session and coordinates storage operations with agent execution.
/// All operations are processed in a background task to avoid blocking.
/// Events are sent to a shared channel for centralized UI dispatch.
pub struct ConversationManager<S: StorageTypes> {
    conversation_id: ConversationId,
    session: Arc<Mutex<Session<S>>>,
    coordinator: Arc<StorageCoordinator<S>>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    cmd_tx: mpsc::UnboundedSender<ManagerCommand>,
    model: Arc<dyn ChatModel + Send + Sync>,
    /// Full model ID in provider/model format (e.g., "gemini/gemini-3-flash-preview")
    model_id: String,
    #[allow(dead_code)]
    task_handle: JoinHandle<()>,
}

impl<S: StorageTypes> ConversationManager<S> {
    /// Create a new ConversationManager for a conversation
    ///
    /// The `event_tx` is a shared channel sender - events are sent as `(ConversationId, ManagerEvent)`
    /// tuples to allow centralized dispatch to UI.
    ///
    /// `model_id` should be the full model ID in `provider/model` format (e.g., "gemini/gemini-3-flash-preview")
    pub fn new(
        session: Session<S>,
        coordinator: Arc<StorageCoordinator<S>>,
        model: Arc<dyn ChatModel + Send + Sync>,
        model_id: String,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
        user_id: UserId,
        event_tx: SharedEventSender,
    ) -> Self {
        let conversation_id = session.conversation_id().clone();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let session = Arc::new(Mutex::new(session));
        let session_clone = Arc::clone(&session);
        let coordinator_clone = Arc::clone(&coordinator);
        let mcp_registry_clone = Arc::clone(&mcp_registry);
        let initial_model = Arc::clone(&model);
        let conversation_id_clone = conversation_id.clone();
        let model_id_clone = model_id.clone();

        let task_handle = tokio::spawn(async move {
            Self::background_loop(
                conversation_id_clone,
                session_clone,
                coordinator_clone,
                initial_model,
                model_id_clone,
                mcp_registry_clone,
                document_resolver,
                user_id,
                cmd_rx,
                event_tx,
            )
            .await;
        });

        Self {
            conversation_id,
            session,
            coordinator,
            mcp_registry,
            cmd_tx,
            model,
            model_id,
            task_handle,
        }
    }

    async fn background_loop(
        conversation_id: ConversationId,
        session: Arc<Mutex<Session<S>>>,
        coordinator: Arc<StorageCoordinator<S>>,
        mut model: Arc<dyn ChatModel + Send + Sync>,
        mut model_id: String,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
        user_id: UserId,
        mut cmd_rx: mpsc::UnboundedReceiver<ManagerCommand>,
        event_tx: SharedEventSender,
    ) {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                ManagerCommand::SendMessage { content, tool_config } => {
                    // Step 1: Store user input and add to pending
                    let add_result = Self::store_and_add_user_message(
                        &session,
                        &coordinator,
                        content,
                    ).await;

                    match add_result {
                        Ok(user_msg) => {
                            // Emit user message for immediate UI feedback
                            let _ = event_tx.send((conversation_id.clone(), ManagerEvent::UserMessageAdded(user_msg)));

                            // Step 2: Commit user message first (creates the user turn)
                            // This is needed so spawn_agent has a valid parent turn
                            let commit_result = Self::commit_pending(
                                &conversation_id,
                                &session,
                                &coordinator,
                                Some(model.id()),
                                &CommitMode::NewTurns,
                            ).await;

                            match commit_result {
                                Ok(Some((turn_id, span_id))) => {
                                    // Build execution context and run agent
                                    let exec_ctx = ExecutionContext::with_all(
                                        user_id.clone(),
                                        conversation_id.clone(),
                                        turn_id,
                                        Some(span_id),
                                        model_id.clone(),
                                    );

                                    Self::run_agent_and_commit(
                                        &conversation_id,
                                        &session,
                                        &coordinator,
                                        &mcp_registry,
                                        &document_resolver,
                                        exec_ctx,
                                        &model,
                                        tool_config,
                                        CommitMode::NewTurns,
                                        &event_tx,
                                    ).await;
                                }
                                Ok(None) => {
                                    // No pending messages to commit (shouldn't happen)
                                    let _ = event_tx.send((conversation_id.clone(), ManagerEvent::Error("No user message to commit".to_string())));
                                }
                                Err(e) => {
                                    let _ = event_tx.send((conversation_id.clone(), ManagerEvent::Error(format!("Failed to commit user message: {}", e))));
                                }
                            }
                        }
                        Err(e) => {
                            let _ = event_tx.send((conversation_id.clone(), ManagerEvent::Error(format!("Failed to add message: {}", e))));
                        }
                    }
                }

                ManagerCommand::RunAgent { tool_config, commit_mode } => {
                    // For regeneration, the turn already exists
                    // Get the turn_id from commit_mode if it's AtTurn
                    let exec_ctx = if let CommitMode::AtTurn(ref turn_id) = commit_mode {
                        // Create span for the regeneration
                        if let Ok(span_id) = coordinator.create_and_select_span(&conversation_id, turn_id, Some(model.id())).await {
                            ExecutionContext::with_all(
                                user_id.clone(),
                                conversation_id.clone(),
                                turn_id.clone(),
                                Some(span_id),
                                model_id.clone(),
                            )
                        } else {
                            ExecutionContext::default()
                        }
                    } else {
                        // For NewTurns mode without prior commit, context may be incomplete
                        // This case is less common - typically used after truncate
                        ExecutionContext::default()
                    };

                    Self::run_agent_and_commit(
                        &conversation_id,
                        &session,
                        &coordinator,
                        &mcp_registry,
                        &document_resolver,
                        exec_ctx,
                        &model,
                        tool_config,
                        commit_mode,
                        &event_tx,
                    ).await;
                }

                ManagerCommand::Truncate(turn_id) => {
                    let mut sess = session.lock().await;
                    sess.truncate(turn_id.as_ref());
                    let _ = event_tx.send((conversation_id.clone(), ManagerEvent::Truncated(turn_id)));
                }

                ManagerCommand::SetModel { model: new_model, model_id: new_model_id } => {
                    let name = new_model.name().to_string();
                    model = new_model;
                    model_id = new_model_id;
                    let _ = event_tx.send((conversation_id.clone(), ManagerEvent::ModelChanged(name)));
                }
            }
        }
    }

    /// Store user input content and add to session pending
    async fn store_and_add_user_message(
        session: &Arc<Mutex<Session<S>>>,
        coordinator: &Arc<StorageCoordinator<S>>,
        content: Vec<InputContent>,
    ) -> Result<ChatMessage> {
        if content.is_empty() {
            anyhow::bail!("Empty content");
        }

        // Store content and get refs
        let stored = coordinator
            .store_input_content(content, OriginKind::User)
            .await?;

        // Resolve refs to ContentBlocks
        let mut blocks = Vec::with_capacity(stored.len());
        for item in stored {
            let block = item.resolve(coordinator.as_ref()).await?;
            blocks.push(block);
        }

        // Create ChatMessage and add to pending
        let message = ChatMessage::user(ChatPayload::new(blocks));
        {
            let mut sess = session.lock().await;
            sess.add(message.clone());
        }

        Ok(message)
    }

    /// Run agent and commit results
    async fn run_agent_and_commit(
        conversation_id: &ConversationId,
        session: &Arc<Mutex<Session<S>>>,
        coordinator: &Arc<StorageCoordinator<S>>,
        mcp_registry: &Arc<Mutex<McpRegistry>>,
        document_resolver: &Arc<dyn DocumentResolver>,
        execution_context: ExecutionContext,
        model: &Arc<dyn ChatModel + Send + Sync>,
        tool_config: ToolConfig,
        commit_mode: CommitMode,
        event_tx: &SharedEventSender,
    ) {
        // Create agent with enricher for noema-core tools
        let tool_registry = McpToolRegistry::new(Arc::clone(mcp_registry));
        let agent = McpAgent::with_enricher(
            Arc::new(tool_registry),
            10,
            Arc::clone(document_resolver),
            execution_context,
            create_noema_core_enricher(),
        );

        // Run agent
        let execute_result = {
            let mut sess = session.lock().await;
            if tool_config.enabled {
                agent.execute_stream(&mut *sess, model.clone()).await
            } else {
                agent.execute_stream_no_tools(&mut *sess, model.clone()).await
            }
        };

        match execute_result {
            Ok(_) => {
                // Send streaming messages
                {
                    let sess = session.lock().await;
                    for msg in sess.pending() {
                        // Skip user messages (already sent)
                        if msg.role != llm::Role::User {
                            let _ = event_tx.send((conversation_id.clone(), ManagerEvent::StreamingMessage(msg.clone())));
                        }
                    }
                }

                // Commit pending messages (assistant messages)
                let commit_result = Self::commit_pending(
                    conversation_id,
                    session,
                    coordinator,
                    Some(model.id()),
                    &commit_mode,
                ).await;

                match commit_result {
                    Ok(_) => {
                        // Get all resolved messages for complete event (includes turn_ids)
                        let messages = {
                            let sess = session.lock().await;
                            sess.messages_for_display().to_vec()
                        };
                        let _ = event_tx.send((conversation_id.clone(), ManagerEvent::Complete(messages)));
                    }
                    Err(e) => {
                        let _ = event_tx.send((conversation_id.clone(), ManagerEvent::Error(format!("Failed to commit: {}", e))));
                    }
                }
            }
            Err(e) => {
                let _ = event_tx.send((conversation_id.clone(), ManagerEvent::Error(e.to_string())));
            }
        }
    }

    /// Commit pending messages to storage
    /// Returns the (turn_id, span_id) of the first turn committed, if any
    async fn commit_pending(
        conversation_id: &ConversationId,
        session: &Arc<Mutex<Session<S>>>,
        coordinator: &Arc<StorageCoordinator<S>>,
        model_id: Option<&str>,
        commit_mode: &CommitMode,
    ) -> Result<Option<(TurnId, SpanId)>> {
        let mut sess = session.lock().await;

        if sess.pending().is_empty() {
            return Ok(None);
        }

        let pending: Vec<ChatMessage> = sess.pending().to_vec();

        // Track current turn and span for adding messages
        let mut current_turn: Option<TurnId> = None;
        let mut current_span: Option<SpanId> = None;
        let mut current_role = None;
        let mut first_turn_info: Option<(TurnId, SpanId)> = None;

        for msg in pending {
            let origin = OriginKind::from(msg.role);
            let span_role = msg.role;

            // Get or create turn and span based on commit mode
            let (turn_id, span_id) = match commit_mode {
                CommitMode::AtTurn(tid) => {
                    if current_span.is_none() {
                        let span = coordinator
                            .create_and_select_span(conversation_id, tid, model_id)
                            .await?;
                        current_turn = Some(tid.clone());
                        current_span = Some(span);
                    }
                    (current_turn.as_ref().unwrap().clone(), current_span.as_ref().unwrap().clone())
                }
                CommitMode::NewTurns => {
                    if current_role != Some(span_role) {
                        let tid = coordinator.create_turn(span_role).await?;
                        let span = coordinator
                            .create_and_select_span(conversation_id, &tid, model_id)
                            .await?;
                        current_turn = Some(tid);
                        current_span = Some(span);
                        current_role = Some(span_role);
                    }
                    (current_turn.as_ref().unwrap().clone(), current_span.as_ref().unwrap().clone())
                }
            };

            // Track first turn info for return value
            if first_turn_info.is_none() {
                first_turn_info = Some((turn_id.clone(), span_id.clone()));
            }

            let resolved = coordinator
                .add_message(&span_id, &turn_id, msg.role, msg.payload.content, origin)
                .await?;
            sess.add_resolved(resolved);
        }

        sess.clear_pending();
        Ok(first_turn_info)
    }

    // ========================================================================
    // Public API
    // ========================================================================

    /// Send a user message
    pub fn send_message(&self, content: Vec<InputContent>, tool_config: ToolConfig) {
        let _ = self.cmd_tx.send(ManagerCommand::SendMessage { content, tool_config });
    }

    /// Regenerate response at a turn
    pub fn regenerate(&self, turn_id: TurnId, tool_config: ToolConfig) {
        let _ = self.cmd_tx.send(ManagerCommand::Truncate(Some(turn_id.clone())));
        let _ = self.cmd_tx.send(ManagerCommand::RunAgent {
            tool_config,
            commit_mode: CommitMode::AtTurn(turn_id),
        });
    }

    /// Run agent on current pending messages (for edit flow where session already has pending)
    pub fn run_agent(&self, tool_config: ToolConfig) {
        let _ = self.cmd_tx.send(ManagerCommand::RunAgent {
            tool_config,
            commit_mode: CommitMode::NewTurns,
        });
    }

    /// Clear all history
    pub fn clear_history(&self) {
        let _ = self.cmd_tx.send(ManagerCommand::Truncate(None));
    }

    /// Set the model (model_id should be in provider/model format)
    pub fn set_model(&mut self, model: Arc<dyn ChatModel + Send + Sync>, model_id: String) {
        self.model = Arc::clone(&model);
        self.model_id = model_id.clone();
        let _ = self.cmd_tx.send(ManagerCommand::SetModel { model, model_id });
    }

    /// Get conversation ID
    pub fn conversation_id(&self) -> &ConversationId {
        &self.conversation_id
    }

    /// Get current model name
    pub fn model_name(&self) -> &str {
        self.model.name()
    }

    /// Get all messages (committed + pending) for display
    pub async fn all_messages(&self) -> Vec<ChatMessage> {
        self.session.lock().await.all_messages()
    }

    /// Get messages for display with turn_id preserved (for alternates enrichment)
    pub async fn messages_for_display(&self) -> Vec<ResolvedMessage> {
        self.session.lock().await.messages_for_display().to_vec()
    }

    /// Clear the session cache (used when view selection changes)
    pub async fn clear_cache(&self) {
        self.session.lock().await.clear_cache();
    }

    /// Reload messages from storage for current view
    pub async fn reload(&self) -> Result<()> {
        self.session.lock().await.reload().await
    }
}
