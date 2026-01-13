//! ConversationManager - orchestrates storage, session, and agent execution
//!
//! This is the main API for managing a conversation. It coordinates:
//! - Storage operations (storing user input, committing messages)
//! - Session state (pending messages, cache)
//! - Agent execution in a background task
//! - Event streaming to UI

use anyhow::Result;
use llm::{ChatMessage, ChatModel, ChatPayload, Role};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::context::ConversationContext;
use crate::storage::content::InputContent;
use crate::storage::coordinator::StorageCoordinator;
use crate::storage::ids::{ConversationId, TurnId, ViewId};
use crate::storage::session::{ResolvedMessage, Session};
use crate::storage::traits::StorageTypes;
use crate::storage::types::OriginKind;
use crate::storage::DocumentResolver;
use crate::{Agent, McpAgent, McpRegistry, McpToolRegistry};

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
    /// Change the model
    SetModel(Arc<dyn ChatModel + Send + Sync>),
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
    #[allow(dead_code)]
    task_handle: JoinHandle<()>,
}

impl<S: StorageTypes> ConversationManager<S> {
    /// Create a new ConversationManager for a conversation
    ///
    /// The `event_tx` is a shared channel sender - events are sent as `(ConversationId, ManagerEvent)`
    /// tuples to allow centralized dispatch to UI.
    pub fn new(
        session: Session<S>,
        coordinator: Arc<StorageCoordinator<S>>,
        model: Arc<dyn ChatModel + Send + Sync>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
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

        let task_handle = tokio::spawn(async move {
            Self::background_loop(
                conversation_id_clone,
                session_clone,
                coordinator_clone,
                initial_model,
                mcp_registry_clone,
                document_resolver,
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
            task_handle,
        }
    }

    async fn background_loop(
        conversation_id: ConversationId,
        session: Arc<Mutex<Session<S>>>,
        coordinator: Arc<StorageCoordinator<S>>,
        mut model: Arc<dyn ChatModel + Send + Sync>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
        mut cmd_rx: mpsc::UnboundedReceiver<ManagerCommand>,
        event_tx: SharedEventSender,
    ) {
        // Create agent with MCP tool registry
        let tool_registry = McpToolRegistry::new(Arc::clone(&mcp_registry));
        let agent = McpAgent::new(Arc::new(tool_registry), 10, document_resolver);

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

                            // Step 2: Run agent
                            Self::run_agent_and_commit(
                                &conversation_id,
                                &session,
                                &coordinator,
                                &agent,
                                &model,
                                tool_config,
                                CommitMode::NewTurns,
                                &event_tx,
                            ).await;
                        }
                        Err(e) => {
                            let _ = event_tx.send((conversation_id.clone(), ManagerEvent::Error(format!("Failed to add message: {}", e))));
                        }
                    }
                }

                ManagerCommand::RunAgent { tool_config, commit_mode } => {
                    Self::run_agent_and_commit(
                        &conversation_id,
                        &session,
                        &coordinator,
                        &agent,
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

                ManagerCommand::SetModel(new_model) => {
                    let name = new_model.name().to_string();
                    model = new_model;
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
        agent: &McpAgent,
        model: &Arc<dyn ChatModel + Send + Sync>,
        tool_config: ToolConfig,
        commit_mode: CommitMode,
        event_tx: &SharedEventSender,
    ) {
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

                // Commit pending messages
                let commit_result = Self::commit_pending(
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
    async fn commit_pending(
        session: &Arc<Mutex<Session<S>>>,
        coordinator: &Arc<StorageCoordinator<S>>,
        model_id: Option<&str>,
        commit_mode: &CommitMode,
    ) -> Result<()> {
        let mut sess = session.lock().await;

        if sess.pending().is_empty() {
            return Ok(());
        }

        let view_id = sess.view_id().clone();
        let pending: Vec<ChatMessage> = sess.pending().to_vec();

        // Track current turn and span for adding messages
        let mut current_turn: Option<TurnId> = None;
        let mut current_span = None;
        let mut current_role = None;

        for msg in pending {
            let origin = OriginKind::from(msg.role);
            let span_role = msg.role;

            // Get or create turn and span based on commit mode
            let (turn_id, span_id) = match commit_mode {
                CommitMode::AtTurn(tid) => {
                    if current_span.is_none() {
                        let span = coordinator
                            .create_and_select_span(&view_id, tid, model_id)
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
                            .create_and_select_span(&view_id, &tid, model_id)
                            .await?;
                        current_turn = Some(tid);
                        current_span = Some(span);
                        current_role = Some(span_role);
                    }
                    (current_turn.as_ref().unwrap().clone(), current_span.as_ref().unwrap().clone())
                }
            };

            let resolved = coordinator
                .add_message(&span_id, &turn_id, msg.role, msg.payload.content, origin)
                .await?;
            sess.add_resolved(resolved);
        }

        sess.clear_pending();
        Ok(())
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

    /// Clear all history
    pub fn clear_history(&self) {
        let _ = self.cmd_tx.send(ManagerCommand::Truncate(None));
    }

    /// Set the model
    pub fn set_model(&mut self, model: Arc<dyn ChatModel + Send + Sync>) {
        self.model = Arc::clone(&model);
        let _ = self.cmd_tx.send(ManagerCommand::SetModel(model));
    }

    /// Get conversation ID
    pub fn conversation_id(&self) -> &ConversationId {
        &self.conversation_id
    }
    
    /// Get current model name
    pub fn model_name(&self) -> &str {
        self.model.name()
    }

    /// Get the current view ID
    pub async fn view_id(&self) -> ViewId {
        self.session.lock().await.view_id().clone()
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
}

// ============================================================================
// Helpers
// ============================================================================

fn llm_role_to_origin(role: llm::Role) -> OriginKind {
    match role {
        llm::Role::User => OriginKind::User,
        llm::Role::Assistant => OriginKind::Assistant,
        llm::Role::System => OriginKind::System,
    }
}
