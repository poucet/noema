//! Session manager — unified agent execution loop.
//!
//! Works with any `ConversationContext`. Storage is optional via a callback.
//! Used for both ephemeral (Lumina) and persistent (Noema) sessions.

use std::sync::Arc;

use async_trait::async_trait;
use llm::{ChatMessage, ChatModel, ChatPayload, ContentBlock, Role};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::agents::ExecutionContext;
use crate::context::ConversationContext;
use crate::manager::{ManagerCommand, ManagerEvent, ToolConfig};
use crate::storage::content::InputContent;
use crate::storage::DocumentResolver;
use crate::{Agent, McpAgent, McpRegistry, McpToolRegistry};

// ============================================================================
// Event sender — keyed by session ID string
// ============================================================================

/// Sends (session_id, event) tuples for centralized dispatch.
pub type SessionEventSender = mpsc::UnboundedSender<(String, ManagerEvent)>;

// ============================================================================
// Storage hook — optional, for persistent sessions
// ============================================================================

/// Hook called after user message addition and after agent execution
/// to persist messages to storage. Implement for persistent sessions.
#[async_trait]
pub trait StorageHook: Send + Sync + 'static {
    /// Store user input and return ContentBlocks (may involve blob storage).
    /// Default: convert inline without storage.
    async fn store_user_input(&self, content: Vec<InputContent>) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(content
            .into_iter()
            .filter_map(|c| c.into_content_block_inline())
            .collect())
    }

    /// Called after user message is added to pending, before agent runs.
    async fn on_user_message(&self, _message: &ChatMessage) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called after agent execution completes successfully.
    /// Pending messages are available for commit.
    async fn on_agent_complete(&self, _context: &mut dyn ConversationContext) -> anyhow::Result<()> {
        Ok(())
    }

    /// Build execution context for agent tools (turn_id, span_id, etc.).
    fn execution_context(&self) -> ExecutionContext {
        ExecutionContext::default()
    }
}

/// No-op storage hook for ephemeral sessions.
pub struct NoStorage;

#[async_trait]
impl StorageHook for NoStorage {}

// ============================================================================
// Inner state
// ============================================================================

struct Inner<C: ConversationContext> {
    session_id: String,
    session: Mutex<C>,
    model: Mutex<Arc<dyn ChatModel + Send + Sync>>,
    model_id: Mutex<String>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    document_resolver: Arc<dyn DocumentResolver>,
    event_tx: SessionEventSender,
    storage: Box<dyn StorageHook>,
}

impl<C: ConversationContext> Inner<C> {
    fn emit(&self, event: ManagerEvent) {
        let _ = self.event_tx.send((self.session_id.clone(), event));
    }
}

// ============================================================================
// SessionManager
// ============================================================================

/// Unified session manager. Works with any ConversationContext.
/// Storage is pluggable via `StorageHook`.
pub struct SessionManager {
    session_id: String,
    cmd_tx: mpsc::UnboundedSender<ManagerCommand>,
    _task: JoinHandle<()>,
}

impl SessionManager {
    /// Create a new session manager.
    ///
    /// - `session_id`: opaque ID for event routing
    /// - `context`: the conversation context (LightSession, Session, etc.)
    /// - `storage`: storage hook (use `NoStorage` for ephemeral)
    pub fn new<C: ConversationContext + 'static>(
        session_id: String,
        context: C,
        model: Arc<dyn ChatModel + Send + Sync>,
        model_id: String,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
        event_tx: SessionEventSender,
        storage: impl StorageHook,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let inner = Arc::new(Inner {
            session_id: session_id.clone(),
            session: Mutex::new(context),
            model: Mutex::new(model),
            model_id: Mutex::new(model_id),
            mcp_registry,
            document_resolver,
            event_tx,
            storage: Box::new(storage),
        });

        let task = tokio::spawn(background_loop(inner, cmd_rx));

        Self {
            session_id,
            cmd_tx,
            _task: task,
        }
    }

    pub fn send_message(&self, content: Vec<InputContent>, tool_config: ToolConfig) {
        let _ = self
            .cmd_tx
            .send(ManagerCommand::SendMessage { content, tool_config });
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

async fn background_loop<C: ConversationContext + 'static>(
    inner: Arc<Inner<C>>,
    mut cmd_rx: mpsc::UnboundedReceiver<ManagerCommand>,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            ManagerCommand::SendMessage { content, tool_config } => {
                handle_send_message(&inner, content, tool_config).await;
            }
            ManagerCommand::RunAgent { tool_config, .. } => {
                handle_run_agent(&inner, tool_config).await;
            }
            ManagerCommand::SetModel { model, model_id } => {
                *inner.model.lock().await = model;
                let name = model_id.clone();
                *inner.model_id.lock().await = model_id;
                inner.emit(ManagerEvent::ModelChanged(name));
            }
        }
    }
}

async fn handle_send_message<C: ConversationContext>(
    inner: &Arc<Inner<C>>,
    content: Vec<InputContent>,
    tool_config: ToolConfig,
) {
    // Convert input to content blocks (storage hook may store blobs)
    let blocks = match inner.storage.store_user_input(content).await {
        Ok(b) if b.is_empty() => {
            inner.emit(ManagerEvent::Error("empty message".into()));
            return;
        }
        Ok(b) => b,
        Err(e) => {
            inner.emit(ManagerEvent::Error(format!("Failed to store input: {e}")));
            return;
        }
    };

    let user_msg = ChatMessage::user(ChatPayload::new(blocks));
    inner.session.lock().await.add(user_msg.clone());

    if let Err(e) = inner.storage.on_user_message(&user_msg).await {
        inner.emit(ManagerEvent::Error(format!("Storage error: {e}")));
        return;
    }

    inner.emit(ManagerEvent::UserMessageAdded(user_msg));

    handle_run_agent(inner, tool_config).await;
}

async fn handle_run_agent<C: ConversationContext>(
    inner: &Arc<Inner<C>>,
    tool_config: ToolConfig,
) {
    let exec_ctx = inner.storage.execution_context();
    let model = inner.model.lock().await.clone();

    // Create agent and run
    let tool_registry = McpToolRegistry::new(Arc::clone(&inner.mcp_registry));
    let agent = McpAgent::new(
        Arc::new(tool_registry),
        10,
        Arc::clone(&inner.document_resolver),
        exec_ctx,
    );

    let result = {
        let mut sess = inner.session.lock().await;
        if tool_config.enabled {
            agent.execute_stream(&mut *sess, model).await
        } else {
            agent.execute_stream_no_tools(&mut *sess, model).await
        }
    };

    match result {
        Ok(_) => {
            // Emit assistant messages
            {
                let sess = inner.session.lock().await;
                for msg in sess.pending() {
                    if msg.role != Role::User {
                        inner.emit(ManagerEvent::StreamingMessage(msg.clone()));
                    }
                }
            }

            // Let storage hook commit if it wants
            {
                let mut sess = inner.session.lock().await;
                if let Err(e) = inner.storage.on_agent_complete(&mut *sess).await {
                    inner.emit(ManagerEvent::Error(format!("Storage commit failed: {e}")));
                    return;
                }
            }

            inner.emit(ManagerEvent::Complete(vec![]));
        }
        Err(e) => {
            inner.emit(ManagerEvent::Error(e.to_string()));
        }
    }
}
