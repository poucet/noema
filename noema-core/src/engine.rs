//! Chat engine for managing conversation sessions
//!
//! The engine coordinates:
//! - Session (which implements ConversationContext)
//! - Agent execution for LLM interactions
//! - Content storage via the coordinator
//! - Tool execution via MCP registry

use crate::{Agent, ConversationContext, McpAgent, McpRegistry, McpToolRegistry};
use crate::storage::session::Session;
use crate::storage::traits::TurnStore;
use llm::{ChatMessage, ChatModel, ChatPayload};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

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

pub enum EngineCommand {
    SendMessage(ChatPayload, ToolConfig),
    SetModel(Arc<dyn ChatModel + Send + Sync>),
    ClearHistory,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Message(ChatMessage),
    MessageComplete,
    Error(String),
    ModelChanged(String),
    HistoryCleared,

    // Parallel execution events
    ParallelStreamingMessage {
        model_id: String,
        message: ChatMessage,
    },
    ParallelModelComplete {
        model_id: String,
        messages: Vec<ChatMessage>,
    },
    ParallelComplete {
        turn_id: String,
        alternates: Vec<ParallelAlternateInfo>,
    },
    ParallelModelError {
        model_id: String,
        error: String,
    },
}

#[derive(Debug, Clone)]
pub struct ParallelAlternateInfo {
    pub span_id: String,
    pub model_id: String,
    pub model_display_name: String,
    pub message_count: usize,
    pub is_selected: bool,
}

/// Chat engine that manages conversation sessions
///
/// Uses Session<S> which implements ConversationContext directly.
pub struct ChatEngine<S: TurnStore + Send + Sync + 'static> {
    session: Arc<Mutex<Session<S>>>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    cmd_tx: mpsc::UnboundedSender<EngineCommand>,
    event_rx: mpsc::UnboundedReceiver<EngineEvent>,
    model: Arc<dyn ChatModel + Send + Sync>,
    #[allow(dead_code)]
    processor_handle: JoinHandle<()>,
    #[allow(dead_code)]
    document_resolver: Arc<dyn DocumentResolver>,
}

impl<S: TurnStore + Send + Sync + 'static> ChatEngine<S> {
    pub fn new(
        session: Session<S>,
        model: Arc<dyn ChatModel + Send + Sync>,
        mcp_registry: McpRegistry,
        document_resolver: Arc<dyn DocumentResolver>,
    ) -> Self {
        let session = Arc::new(Mutex::new(session));
        let mcp_registry = Arc::new(Mutex::new(mcp_registry));
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let session_clone = Arc::clone(&session);
        let mcp_registry_clone = Arc::clone(&mcp_registry);
        let initial_model = Arc::clone(&model);
        let resolver_clone = Arc::clone(&document_resolver);

        let processor_handle = tokio::spawn(async move {
            Self::processor_loop(
                session_clone,
                initial_model,
                mcp_registry_clone,
                resolver_clone,
                cmd_rx,
                event_tx,
            )
            .await;
        });

        Self {
            session,
            mcp_registry,
            cmd_tx,
            event_rx,
            model,
            processor_handle,
            document_resolver,
        }
    }

    async fn processor_loop(
        session: Arc<Mutex<Session<S>>>,
        mut model: Arc<dyn ChatModel + Send + Sync>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
        mut cmd_rx: mpsc::UnboundedReceiver<EngineCommand>,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) {
        let tool_registry = McpToolRegistry::new(Arc::clone(&mcp_registry));
        let agent = McpAgent::new(Arc::new(tool_registry), 10, Arc::clone(&document_resolver));

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                EngineCommand::SendMessage(payload, tool_config) => {
                    // Add user message to session
                    {
                        let mut sess = session.lock().await;
                        sess.add(ChatMessage::user(payload));
                    }

                    // Run agent with session as context
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
                            // Send pending messages to UI
                            let sess = session.lock().await;
                            for msg in sess.pending() {
                                let _ = event_tx.send(EngineEvent::Message(msg.clone()));
                            }
                            let _ = event_tx.send(EngineEvent::MessageComplete);

                            // Note: Actual commit to storage requires ContentStorer
                            // which the engine doesn't have. The caller should handle
                            // committing after receiving MessageComplete.
                        }
                        Err(e) => {
                            let _ = event_tx.send(EngineEvent::Error(e.to_string()));
                        }
                    }
                }
                EngineCommand::SetModel(new_model) => {
                    let name = new_model.name().to_string();
                    model = new_model;
                    let _ = event_tx.send(EngineEvent::ModelChanged(name));
                }
                EngineCommand::ClearHistory => {
                    let mut sess = session.lock().await;
                    sess.clear_cache();
                    sess.clear_pending();
                    let _ = event_tx.send(EngineEvent::HistoryCleared);
                }
            }
        }
    }

    pub fn send_message(&self, payload: impl Into<ChatPayload>, tool_config: ToolConfig) {
        let _ = self.cmd_tx.send(EngineCommand::SendMessage(payload.into(), tool_config));
    }

    pub fn set_model(&mut self, model: Arc<dyn ChatModel + Send + Sync>) {
        self.model = Arc::clone(&model);
        let _ = self.cmd_tx.send(EngineCommand::SetModel(model));
    }

    pub fn clear_history(&self) {
        let _ = self.cmd_tx.send(EngineCommand::ClearHistory);
    }

    pub fn try_recv(&mut self) -> Option<EngineEvent> {
        match self.event_rx.try_recv() {
            Ok(event) => Some(event),
            Err(_) => None,
        }
    }

    pub async fn next_event(&mut self) -> Option<EngineEvent> {
        self.event_rx.recv().await
    }

    pub fn get_session(&self) -> Arc<Mutex<Session<S>>> {
        Arc::clone(&self.session)
    }

    pub fn get_mcp_registry(&self) -> Arc<Mutex<McpRegistry>> {
        Arc::clone(&self.mcp_registry)
    }

    pub fn get_model_name(&self) -> &str {
        self.model.name()
    }
}
