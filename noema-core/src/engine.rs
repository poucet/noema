use crate::{Agent, McpAgent, McpRegistry, McpToolRegistry, Session};
use llm::{ChatMessage, ChatModel, ChatPayload};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

pub enum EngineCommand {
    SendMessage(String),
    SetModel {
        model: Arc<dyn ChatModel + Send + Sync>,
        name: String,
    },
    ClearHistory,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Token(String),
    MessageComplete,
    Error(String),
    ModelChanged(String),
    HistoryCleared,
}

pub struct ChatEngine {
    session: Arc<Mutex<Session>>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    cmd_tx: mpsc::UnboundedSender<EngineCommand>,
    event_rx: mpsc::UnboundedReceiver<EngineEvent>,
    model_name: String,
}

impl ChatEngine {
    pub fn new(
        session: Session,
        model: Arc<dyn ChatModel + Send + Sync>,
        model_name: String,
        mcp_registry: McpRegistry,
    ) -> Self {
        let session = Arc::new(Mutex::new(session));
        let mcp_registry = Arc::new(Mutex::new(mcp_registry));
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let session_clone = Arc::clone(&session);
        let mcp_registry_clone = Arc::clone(&mcp_registry);
        let initial_model = Arc::clone(&model);

        tokio::spawn(async move {
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
            model_name,
        }
    }

    async fn processor_loop(
        session: Arc<Mutex<Session>>,
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
                EngineCommand::SendMessage(message) => {
                    // 1. Lock Session and add user message immediately
                    // This allows UI to read the updated history while agent is running
                    let mut tx = {
                        let mut sess = session.lock().await;
                        // Manually add user message to session
                        sess.messages_mut().push(ChatMessage::user(ChatPayload::text(&message)));
                        // Begin transaction (this clones the history including the new user message)
                        sess.begin()
                    };

                    // 2. Run Agent (streaming) WITHOUT holding Session lock
                    // The agent operates on the transaction (which has a snapshot of history)
                    // and adds new messages to tx.pending
                    match agent.execute_stream(&mut tx, model.clone()).await {
                        Ok(_) => {
                            // 3. Send pending messages (tokens are handled inside execute_stream/send_stream usually?)
                            // Wait, execute_stream adds FULL messages or chunks?
                            // Standard Agent::execute_stream usually streams to the Model, receives chunks,
                            // and might add them to context?
                            // If Agent implementation streams chunks via a callback, we need to hook into it.
                            // But Agent trait is: async fn execute_stream(&self, context: &mut impl Context, model: Arc<dyn Model>)
                            // It assumes context handles partials?
                            // Transaction supports adding messages. It doesn't support "streaming tokens".
                            
                            // Re-reading McpAgent/SimpleAgent:
                            // They usually call model.stream_chat() and iterate chunks.
                            // Then they reconstruct the message and context.add(full_message).
                            
                            // If we want real-time token streaming to UI, we need a side-channel or Context support for tokens.
                            // But EngineEvent::Token is what UI expects.
                            // Current implementations might NOT be sending tokens if we just use execute_stream?
                            
                            // Let's check how `sess.send_stream` worked before.
                            // It returned a `Transaction`. It didn't return a stream of tokens.
                            // The `processor_loop` in `tui/src/main.rs` was:
                            // match sess.send_stream(...) { Ok(tx) => { ... pending = tx.pending() ... } }
                            // This implies `send_stream` COMPLETED and returned pending messages.
                            // IT WAS NOT STREAMING TO UI!
                            // The previous code was "send_stream" (creates transaction, runs agent stream, returns transaction).
                            // Then it iterated pending messages and sent them as chunks? 
                            // No, `tx.pending()` returns `Vec<ChatMessage>`.
                            // So it was sending FULL MESSAGES as "chunks" to `MessageResponse::Chunk`.
                            
                            // If we want true streaming (tokens), we need `Agent` to support a callback or channel.
                            // But sticking to previous behavior:
                            // We have `tx` with pending messages (the response).
                            
                            let pending = tx.pending();
                            for msg in pending.iter() {
                                let text = msg.get_text();
                                let _ = event_tx.send(EngineEvent::Token(text));
                            }
                            
                            // 4. Commit transaction
                            let mut sess = session.lock().await;
                            match sess.commit(tx).await {
                                Ok(_) => { let _ = event_tx.send(EngineEvent::MessageComplete); }
                                Err(e) => { let _ = event_tx.send(EngineEvent::Error(format!("Commit failed: {}", e))); }
                            }
                        }
                        Err(e) => { let _ = event_tx.send(EngineEvent::Error(e.to_string())); }
                    }
                }
                EngineCommand::SetModel { model: new_model, name } => {
                    model = new_model;
                    let _ = event_tx.send(EngineEvent::ModelChanged(name));
                }
                EngineCommand::ClearHistory => {
                     let mut sess = session.lock().await;
                     sess.clear();
                     let _ = event_tx.send(EngineEvent::HistoryCleared);
                }
            }
        }
    }

    pub fn send_message(&self, message: String) {
        let _ = self.cmd_tx.send(EngineCommand::SendMessage(message));
    }

    pub fn set_model(&mut self, model: Arc<dyn ChatModel + Send + Sync>, name: String) {
        self.model_name = name.clone();
        let _ = self.cmd_tx.send(EngineCommand::SetModel { model, name });
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
    
    pub fn get_session(&self) -> Arc<Mutex<Session>> {
        Arc::clone(&self.session)
    }
    
    pub fn get_mcp_registry(&self) -> Arc<Mutex<McpRegistry>> {
        Arc::clone(&self.mcp_registry)
    }
    
    pub fn get_model_name(&self) -> &str {
        &self.model_name
    }
}