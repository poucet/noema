//! Unified session manager — decoupled from storage.
//!
//! The core loop: receive user message → run agent → stream events.
//! Storage is handled by an external subscriber via `StorageHook`.
//! Tool approval is handled at the MCP layer by the daemon, not here.

use std::sync::Arc;

use async_trait::async_trait;
use llm::{ChatMessage, ChatModel, ChatPayload, ContentBlock, Role};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::agents::mcp_agent::{AgentStreamEvent, AgentStreamSink};
use crate::agents::ExecutionContext;
use crate::context::ConversationContext;
use crate::storage::content::InputContent;
use crate::storage::DocumentResolver;
use crate::{Agent, McpAgent, McpRegistry, McpToolRegistry};

// ============================================================================
// Events
// ============================================================================

/// Events emitted by the session manager.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// User message added (immediate feedback).
    UserMessageAdded(ChatMessage),
    /// Text delta streamed from LLM (real-time).
    TextDelta(String),
    /// Non-text content block from LLM (image, audio, etc.).
    ContentBlock(ContentBlock),
    /// Complete assistant message (after streaming finishes for one agent iteration).
    AssistantMessage(ChatMessage),
    /// Turn completed.
    TurnComplete {
        /// New messages added during this turn.
        new_messages: Vec<ChatMessage>,
    },
    /// Error.
    Error(String),
    /// Model changed.
    ModelChanged(String),
}

// ============================================================================
// Commands
// ============================================================================

/// Commands sent to the session manager.
pub enum SessionCommand {
    /// Send a user message and run the agent.
    SendMessage {
        content: Vec<InputContent>,
        tools_enabled: bool,
    },
    /// Change the model.
    SetModel {
        model: Arc<dyn ChatModel + Send + Sync>,
        model_id: String,
    },
}

/// Sends `(session_id, event)` for centralized dispatch.
pub type SessionEventSender = mpsc::UnboundedSender<(String, SessionEvent)>;

// ============================================================================
// Storage Hook
// ============================================================================

/// Optional hook for storage. Ephemeral sessions use `NoStorage`.
#[async_trait]
pub trait StorageHook: Send + Sync + 'static {
    /// Convert user input to content blocks. May store blobs for persistent sessions.
    async fn store_user_input(
        &self,
        content: Vec<InputContent>,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(content
            .into_iter()
            .filter_map(|c| c.into_content_block_inline())
            .collect())
    }

    /// Called when a turn completes with all new messages.
    async fn on_turn_complete(&self, _new_messages: &[ChatMessage]) -> anyhow::Result<()> {
        Ok(())
    }
}

/// No-op storage for ephemeral sessions.
pub struct NoStorage;

#[async_trait]
impl StorageHook for NoStorage {}

// ============================================================================
// Inner state
// ============================================================================

struct Inner<C: ConversationContext> {
    session_id: String,
    context: Mutex<C>,
    model: Mutex<Arc<dyn ChatModel + Send + Sync>>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    document_resolver: Arc<dyn DocumentResolver>,
    event_tx: SessionEventSender,
    storage: Box<dyn StorageHook>,
}

impl<C: ConversationContext> Inner<C> {
    fn emit(&self, event: SessionEvent) {
        let _ = self.event_tx.send((self.session_id.clone(), event));
    }
}

// ============================================================================
// SessionManager
// ============================================================================

pub struct SessionManager {
    session_id: String,
    cmd_tx: mpsc::UnboundedSender<SessionCommand>,
    _task: JoinHandle<()>,
}

impl SessionManager {
    pub fn new<C: ConversationContext + 'static>(
        session_id: impl Into<String>,
        context: C,
        model: Arc<dyn ChatModel + Send + Sync>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
        event_tx: SessionEventSender,
        storage: impl StorageHook,
    ) -> Self {
        let session_id = session_id.into();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let inner = Arc::new(Inner {
            session_id: session_id.clone(),
            context: Mutex::new(context),
            model: Mutex::new(model),
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

    pub fn send_message(&self, content: Vec<InputContent>, tools_enabled: bool) {
        let _ = self.cmd_tx.send(SessionCommand::SendMessage {
            content,
            tools_enabled,
        });
    }

    pub fn set_model(&self, model: Arc<dyn ChatModel + Send + Sync>, model_id: String) {
        let _ = self
            .cmd_tx
            .send(SessionCommand::SetModel { model, model_id });
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

// ============================================================================
// Background loop
// ============================================================================

async fn background_loop<C: ConversationContext + 'static>(
    inner: Arc<Inner<C>>,
    mut cmd_rx: mpsc::UnboundedReceiver<SessionCommand>,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            SessionCommand::SendMessage {
                content,
                tools_enabled,
            } => {
                handle_send_message(&inner, content, tools_enabled).await;
            }
            SessionCommand::SetModel { model, model_id } => {
                *inner.model.lock().await = model;
                inner.emit(SessionEvent::ModelChanged(model_id));
            }
        }
    }
}

async fn handle_send_message<C: ConversationContext>(
    inner: &Inner<C>,
    content: Vec<InputContent>,
    tools_enabled: bool,
) {
    // 1. Convert input to content blocks
    let blocks = match inner.storage.store_user_input(content).await {
        Ok(b) if b.is_empty() => {
            inner.emit(SessionEvent::Error("empty message".into()));
            return;
        }
        Ok(b) => b,
        Err(e) => {
            inner.emit(SessionEvent::Error(format!("Failed to process input: {e}")));
            return;
        }
    };

    // 2. Add user message to context, record position
    let user_msg = ChatMessage::user(ChatPayload::new(blocks));
    let msg_count_before = {
        let mut ctx = inner.context.lock().await;
        let count = ctx.len();
        ctx.add(user_msg.clone());
        count
    };
    inner.emit(SessionEvent::UserMessageAdded(user_msg));

    // 3. Set up stream sink to forward text deltas in real-time
    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<AgentStreamEvent>();

    // Spawn a forwarder that converts agent stream events to session events
    let session_id = inner.session_id.clone();
    let event_tx = inner.event_tx.clone();
    let forwarder = tokio::spawn(async move {
        while let Some(evt) = stream_rx.recv().await {
            let session_event = match evt {
                AgentStreamEvent::TextDelta(text) => SessionEvent::TextDelta(text),
                AgentStreamEvent::ContentBlock(block) => SessionEvent::ContentBlock(block),
            };
            let _ = event_tx.send((session_id.clone(), session_event));
        }
    });

    // 4. Run agent with stream sink
    let model = inner.model.lock().await.clone();
    let tool_registry = McpToolRegistry::new(Arc::clone(&inner.mcp_registry));
    let agent = McpAgent::new(
        Arc::new(tool_registry),
        10,
        Arc::clone(&inner.document_resolver),
        ExecutionContext::default(),
    )
    .with_stream_sink(stream_tx);

    let result = {
        let mut ctx = inner.context.lock().await;
        if tools_enabled {
            agent.execute_stream(&mut *ctx, model).await
        } else {
            agent.execute_stream_no_tools(&mut *ctx, model).await
        }
    };

    // Agent is done — drop the sink so the forwarder exits
    drop(agent);
    let _ = forwarder.await;

    match result {
        Ok(_) => {
            // 5. Collect new messages added by the agent
            let new_messages = {
                let mut ctx = inner.context.lock().await;
                match ctx.messages().await {
                    Ok(all) => all
                        .iter()
                        .skip(msg_count_before + 1) // +1 for user msg
                        .cloned()
                        .collect::<Vec<_>>(),
                    Err(_) => vec![],
                }
            };

            // Emit complete assistant messages
            for msg in &new_messages {
                if msg.role != Role::User {
                    inner.emit(SessionEvent::AssistantMessage(msg.clone()));
                }
            }

            // 6. Notify storage hook
            if let Err(e) = inner.storage.on_turn_complete(&new_messages).await {
                inner.emit(SessionEvent::Error(format!("Storage error: {e}")));
                return;
            }

            inner.emit(SessionEvent::TurnComplete { new_messages });
        }
        Err(e) => {
            inner.emit(SessionEvent::Error(e.to_string()));
        }
    }
}
