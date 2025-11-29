use crate::{Agent, ConversationContext, McpAgent, McpRegistry, McpToolRegistry, SessionStore, StorageTransaction};
use llm::{ChatMessage, ChatModel, ChatPayload};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

pub enum EngineCommand {
    SendMessage(ChatPayload),
    SetModel(Arc<dyn ChatModel + Send + Sync>),
    ClearHistory,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// A message chunk (can contain text, images, audio, tool calls, etc.)
    Message(ChatMessage),
    MessageComplete,
    Error(String),
    ModelChanged(String),
    HistoryCleared,
}

/// Chat engine that manages conversation sessions with any storage backend
///
/// Generic over `S: SessionStore` to support both in-memory and persistent storage.
pub struct ChatEngine<S: SessionStore + 'static> {
    session: Arc<Mutex<S>>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    cmd_tx: mpsc::UnboundedSender<EngineCommand>,
    event_rx: mpsc::UnboundedReceiver<EngineEvent>,
    model: Arc<dyn ChatModel + Send + Sync>,
    #[allow(dead_code)]
    processor_handle: JoinHandle<()>,
}

impl<S: SessionStore + 'static> ChatEngine<S>
where
    S::Transaction: Send + 'static,
{
    pub fn new(
        session: S,
        model: Arc<dyn ChatModel + Send + Sync>,
        mcp_registry: McpRegistry,
    ) -> Self {
        let session = Arc::new(Mutex::new(session));
        let mcp_registry = Arc::new(Mutex::new(mcp_registry));
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let session_clone = Arc::clone(&session);
        let mcp_registry_clone = Arc::clone(&mcp_registry);
        let initial_model = Arc::clone(&model);

        let processor_handle = tokio::spawn(async move {
            Self::processor_loop(
                session_clone,
                initial_model,
                mcp_registry_clone,
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
        }
    }

    async fn processor_loop(
        session: Arc<Mutex<S>>,
        mut model: Arc<dyn ChatModel + Send + Sync>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        mut cmd_rx: mpsc::UnboundedReceiver<EngineCommand>,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) {
        // Create dynamic tool registry that queries MCP servers on each call
        let tool_registry = McpToolRegistry::new(Arc::clone(&mcp_registry));
        // Agent is stateless regarding model, but holds tool registry
        let agent = McpAgent::new(Arc::new(tool_registry), 10);

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                EngineCommand::SendMessage(payload) => {
                    // 1. Begin transaction and add user message to it
                    let mut tx = {
                        let sess = session.lock().await;
                        let mut tx = sess.begin();
                        // Add user message to transaction (will be written to DB on commit)
                        tx.add(ChatMessage::user(payload));
                        tx
                    };

                    // 2. Run Agent (streaming) WITHOUT holding Session lock
                    match agent.execute_stream(&mut tx, model.clone()).await {
                        Ok(_) => {
                            // 3. Send pending messages (full multimodal content) to UI
                            let pending = tx.pending();
                            for msg in pending.iter() {
                                let _ = event_tx.send(EngineEvent::Message(msg.clone()));
                            }

                            // 4. Commit transaction
                            let mut sess = session.lock().await;
                            match sess.commit(tx).await {
                                Ok(_) => {
                                    let _ = event_tx.send(EngineEvent::MessageComplete);
                                }
                                Err(e) => {
                                    let _ = event_tx
                                        .send(EngineEvent::Error(format!("Commit failed: {}", e)));
                                }
                            }
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
                    let _ = sess.clear().await;
                    let _ = event_tx.send(EngineEvent::HistoryCleared);
                }
            }
        }
    }

    pub fn send_message(&self, payload: impl Into<ChatPayload>) {
        let _ = self.cmd_tx.send(EngineCommand::SendMessage(payload.into()));
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

    pub fn get_session(&self) -> Arc<Mutex<S>> {
        Arc::clone(&self.session)
    }

    pub fn get_mcp_registry(&self) -> Arc<Mutex<McpRegistry>> {
        Arc::clone(&self.mcp_registry)
    }

    pub fn get_model_name(&self) -> &str {
        self.model.name()
    }
}
