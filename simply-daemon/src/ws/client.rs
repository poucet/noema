//! RemoteDaemon — implements `DaemonApi` over a WebSocket connection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::api::*;
use super::protocol::*;

/// A daemon client that talks to a remote daemon over WebSocket.
pub struct RemoteDaemon {
    write_tx: mpsc::Sender<String>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>>,
    session_senders: Arc<Mutex<HashMap<SessionId, broadcast::Sender<DaemonEvent>>>>,
    next_id: AtomicU64,
    _reader: JoinHandle<()>,
    _writer: JoinHandle<()>,
}

impl RemoteDaemon {
    /// Connect to a running daemon at the given address.
    pub async fn connect(addr: &str) -> anyhow::Result<Arc<Self>> {
        let url = format!("ws://{}", addr);
        let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await?;
        let (ws_sink, ws_source) = ws_stream.split();

        let (write_tx, mut write_rx) = mpsc::channel::<String>(256);
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WsResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let session_senders: Arc<Mutex<HashMap<SessionId, broadcast::Sender<DaemonEvent>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Writer task
        let writer = tokio::spawn(async move {
            let mut sink = ws_sink;
            while let Some(text) = write_rx.recv().await {
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        });

        // Reader task — dispatches responses and notifications
        let pending_clone = Arc::clone(&pending);
        let senders_clone = Arc::clone(&session_senders);
        let reader = tokio::spawn(async move {
            let mut source = ws_source;
            while let Some(msg) = source.next().await {
                let text = match msg {
                    Ok(Message::Text(t)) => t.to_string(),
                    Ok(Message::Close(_)) => break,
                    Ok(_) => continue,
                    Err(_) => break,
                };

                let incoming: WsIncoming = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if incoming.is_response() {
                    let id = incoming.id.unwrap();
                    let response = WsResponse {
                        id,
                        result: incoming.result,
                        error: incoming.error,
                    };
                    if let Some(tx) = pending_clone.lock().await.remove(&id) {
                        let _ = tx.send(response);
                    }
                } else if incoming.is_notification() {
                    let method = incoming.method.as_deref().unwrap();
                    if method == "session.event" {
                        if let Ok(params) = serde_json::from_value::<SessionEventParams>(incoming.params) {
                            let senders = senders_clone.lock().await;
                            if let Some(tx) = senders.get(&params.session_id) {
                                let _ = tx.send(params.event);
                            }
                        }
                    }
                }
            }

            // Connection lost — fail all pending requests
            let mut pending = pending_clone.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(WsResponse::err(0, "connection lost"));
            }
        });

        Ok(Arc::new(Self {
            write_tx,
            pending,
            session_senders,
            next_id: AtomicU64::new(1),
            _reader: reader,
            _writer: writer,
        }))
    }

    /// Send a request and wait for the response.
    async fn call(&self, method: &str, params: impl serde::Serialize) -> anyhow::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let request = WsRequest {
            id,
            method: method.to_string(),
            params: serde_json::to_value(params)?,
        };
        self.write_tx
            .send(serde_json::to_string(&request)?)
            .await
            .map_err(|_| anyhow::anyhow!("connection closed"))?;

        let response = rx.await.map_err(|_| anyhow::anyhow!("connection closed"))?;
        match response.error {
            Some(e) => Err(anyhow::anyhow!(e.message)),
            None => Ok(response.result.unwrap_or(serde_json::Value::Null)),
        }
    }

    /// Register a local broadcast channel for a session.
    async fn register_session(&self, session_id: &SessionId) -> broadcast::Receiver<DaemonEvent> {
        let mut senders = self.session_senders.lock().await;
        if let Some(tx) = senders.get(session_id) {
            return tx.subscribe();
        }
        let (tx, rx) = broadcast::channel(256);
        senders.insert(session_id.clone(), tx);
        rx
    }
}

// ---------------------------------------------------------------------------
// SessionApi
// ---------------------------------------------------------------------------

#[async_trait]
impl SessionApi for RemoteDaemon {
    async fn create_session(&self, options: CreateSessionOptions) -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)> {
        let result = self.call("session.create_session", &options).await?;
        let info: SessionInfo = serde_json::from_value(result)?;
        let rx = self.register_session(&info.id).await;
        Ok((info, rx))
    }

    async fn resume_session(&self, session_id: &SessionId) -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)> {
        let result = self.call("session.resume_session", session_id).await?;
        let info: SessionInfo = serde_json::from_value(result)?;
        let rx = self.register_session(&info.id).await;
        Ok((info, rx))
    }

    async fn subscribe_session(&self, session_id: &SessionId) -> anyhow::Result<broadcast::Receiver<DaemonEvent>> {
        self.call("session.subscribe_session", session_id).await?;
        Ok(self.register_session(session_id).await)
    }

    async fn close_session(&self, session_id: &SessionId) -> anyhow::Result<()> {
        self.call("session.close_session", session_id).await?;
        self.session_senders.lock().await.remove(session_id);
        Ok(())
    }

    async fn close_all_sessions(&self) -> anyhow::Result<()> {
        self.call("session.close_all_sessions", &()).await?;
        self.session_senders.lock().await.clear();
        Ok(())
    }

    async fn seed_context(&self, session_id: &SessionId, messages: Vec<SeedMessage>) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct P<'a> { session_id: &'a SessionId, messages: Vec<SeedMessage> }
        self.call("session.seed_context", &P { session_id, messages }).await?;
        Ok(())
    }

    async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>> {
        let r = self.call("session.list_sessions", &()).await?;
        Ok(serde_json::from_value(r)?)
    }

    async fn set_persistence(&self, session_id: &SessionId, persistence: Persistence) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct P<'a> { session_id: &'a SessionId, persistence: Persistence }
        self.call("session.set_persistence", &P { session_id, persistence }).await?;
        Ok(())
    }

    async fn send_message(&self, session_id: &SessionId, message: UserMessage) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct P<'a> { session_id: &'a SessionId, message: UserMessage }
        self.call("session.send_message", &P { session_id, message }).await?;
        Ok(())
    }

    async fn get_messages(&self, session_id: &SessionId) -> anyhow::Result<Vec<ResolvedMessage>> {
        let r = self.call("session.get_messages", session_id).await?;
        Ok(serde_json::from_value(r)?)
    }

    async fn set_model(&self, session_id: &SessionId, model_id: &str) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct P<'a> { session_id: &'a SessionId, model_id: &'a str }
        self.call("session.set_model", &P { session_id, model_id }).await?;
        Ok(())
    }

    async fn reload(&self, session_id: &SessionId) -> anyhow::Result<()> {
        self.call("session.reload", session_id).await?;
        Ok(())
    }

    async fn push_event(&self, event: InboundEvent) -> anyhow::Result<()> {
        self.call("session.push_event", &event).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ConversationApi
// ---------------------------------------------------------------------------

#[async_trait]
impl ConversationApi for RemoteDaemon {
    async fn create_conversation(&self, name: Option<&str>) -> anyhow::Result<ConversationId> {
        #[derive(serde::Serialize)]
        struct P<'a> { name: Option<&'a str> }
        let r = self.call("conversation.create_conversation", &P { name }).await?;
        Ok(serde_json::from_value(r)?)
    }

    async fn list_conversations(&self) -> anyhow::Result<Vec<ConversationInfo>> {
        let r = self.call("conversation.list_conversations", &()).await?;
        Ok(serde_json::from_value(r)?)
    }

    async fn delete_conversation(&self, id: &ConversationId) -> anyhow::Result<()> {
        self.call("conversation.delete_conversation", id).await?;
        Ok(())
    }

    async fn rename_conversation(&self, id: &ConversationId, name: &str) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct P<'a> { conversation_id: &'a ConversationId, name: &'a str }
        self.call("conversation.rename_conversation", &P { conversation_id: id, name }).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AssetApi
// ---------------------------------------------------------------------------

#[async_trait]
impl AssetApi for RemoteDaemon {
    async fn store_asset(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<AssetId> {
        use base64::Engine;
        #[derive(serde::Serialize)]
        struct P<'a> { data: String, media_type: &'a str }
        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
        let r = self.call("asset.store_asset", &P { data: b64, media_type }).await?;
        Ok(serde_json::from_value(r)?)
    }

    async fn get_blob(&self, hash: &simply_core::storage::types::BlobHash) -> anyhow::Result<Vec<u8>> {
        use base64::Engine;
        let r = self.call("asset.get_blob", hash).await?;
        let b64: String = serde_json::from_value(r)?;
        Ok(base64::engine::general_purpose::STANDARD.decode(&b64)?)
    }
}

// ---------------------------------------------------------------------------
// McpApi
// ---------------------------------------------------------------------------

#[async_trait]
impl McpApi for RemoteDaemon {
    async fn list_mcp_servers(&self) -> anyhow::Result<Vec<McpServerInfo>> {
        let r = self.call("mcp.list_mcp_servers", &()).await?;
        Ok(serde_json::from_value(r)?)
    }
    async fn add_mcp_server(&self, request: AddMcpServerRequest) -> anyhow::Result<()> {
        self.call("mcp.add_mcp_server", &request).await?; Ok(())
    }
    async fn remove_mcp_server(&self, server_id: &str) -> anyhow::Result<()> {
        self.call("mcp.remove_mcp_server", server_id).await?; Ok(())
    }
    async fn connect_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> {
        let r = self.call("mcp.connect_mcp_server", server_id).await?;
        Ok(serde_json::from_value(r)?)
    }
    async fn disconnect_mcp_server(&self, server_id: &str) -> anyhow::Result<()> {
        self.call("mcp.disconnect_mcp_server", server_id).await?; Ok(())
    }
    async fn get_mcp_server_tools(&self, server_id: &str) -> anyhow::Result<Vec<McpToolInfo>> {
        let r = self.call("mcp.get_mcp_server_tools", server_id).await?;
        Ok(serde_json::from_value(r)?)
    }
    async fn test_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> {
        let r = self.call("mcp.test_mcp_server", server_id).await?;
        Ok(serde_json::from_value(r)?)
    }
    async fn update_mcp_server_settings(&self, server_id: &str, request: UpdateMcpServerRequest) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct P<'a> { server_id: &'a str, request: UpdateMcpServerRequest }
        self.call("mcp.update_mcp_server_settings", &P { server_id, request }).await?; Ok(())
    }
    async fn stop_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> {
        self.call("mcp.stop_mcp_retry", server_id).await?; Ok(())
    }
    async fn start_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> {
        self.call("mcp.start_mcp_retry", server_id).await?; Ok(())
    }
}

// ---------------------------------------------------------------------------
// OAuthApi
// ---------------------------------------------------------------------------

#[async_trait]
impl OAuthApi for RemoteDaemon {
    async fn start_oauth(&self, server_id: &str) -> anyhow::Result<OAuthFlowInfo> {
        let r = self.call("oauth.start_oauth", server_id).await?;
        Ok(serde_json::from_value(r)?)
    }
    async fn complete_oauth(&self, server_id: &str, code: &str, state: &str) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct P<'a> { server_id: &'a str, code: &'a str, state: &'a str }
        self.call("oauth.complete_oauth", &P { server_id, code, state }).await?; Ok(())
    }
    async fn complete_oauth_with_code(&self, server_id: &str, code: &str) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct P<'a> { server_id: &'a str, code: &'a str }
        self.call("oauth.complete_oauth_with_code", &P { server_id, code }).await?; Ok(())
    }
    async fn resolve_oauth_state(&self, _state: &str) -> Option<String> {
        None // local lookup — not meaningful over remote
    }
}

// ---------------------------------------------------------------------------
// ModelApi
// ---------------------------------------------------------------------------

#[async_trait]
impl ModelApi for RemoteDaemon {
    async fn list_models(&self) -> anyhow::Result<Vec<llm::ModelInfo>> {
        let r = self.call("model.list_models", &()).await?;
        Ok(serde_json::from_value(r)?)
    }
    async fn list_providers(&self) -> Vec<llm::ProviderInfo> {
        self.call("model.list_providers", &()).await
            .and_then(|r| serde_json::from_value(r).map_err(Into::into))
            .unwrap_or_default()
    }
    async fn default_model_id(&self) -> String {
        self.call("model.default_model_id", &()).await
            .and_then(|r| serde_json::from_value(r).map_err(Into::into))
            .unwrap_or_default()
    }
    async fn set_default_model(&self, model_id: &str) -> anyhow::Result<()> {
        self.call("model.set_default_model", model_id).await?; Ok(())
    }
}

// ---------------------------------------------------------------------------
// VoiceApi
// ---------------------------------------------------------------------------

#[async_trait]
impl VoiceApi for RemoteDaemon {
    async fn voice_connect(&self, _session_id: &SessionId) -> anyhow::Result<VoiceHandle> {
        anyhow::bail!("voice not supported over remote connection")
    }
}
