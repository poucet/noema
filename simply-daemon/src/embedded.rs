//! In-process implementation of the daemon API traits.
//!
//! EmbeddedDaemon holds individual service objects that each implement their
//! API trait directly. SessionApi and ConversationApi are implemented here
//! (tightly coupled to session state). All other traits delegate to services.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, Mutex};

use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::ids::ConversationId;
use simply_core::storage::session::ResolvedMessage;
use simply_core::storage::traits::{StorageTypes, Stores};
use simply_core::storage::DocumentResolver;
use simply_core::{
    McpToolRegistry, Persistence, SessionEvent, SessionEventSender, SessionManager, ToolService,
};

use crate::api::*;
use crate::mcp::{McpService, McpServiceConfig};
use crate::services::*;

// ---------------------------------------------------------------------------
// Session bookkeeping
// ---------------------------------------------------------------------------

struct ManagedSession {
    info: SessionInfo,
    manager: SessionManager,
    event_broadcast: broadcast::Sender<DaemonEvent>,
}

// ---------------------------------------------------------------------------
// EmbeddedDaemon
// ---------------------------------------------------------------------------

pub struct EmbeddedDaemon<S: StorageTypes> {
    // Session + Conversation state (tightly coupled, kept here)
    coordinator: Arc<StorageCoordinator<S>>,
    stores: Arc<dyn Stores<S>>,
    sessions: Mutex<HashMap<SessionId, ManagedSession>>,
    user_id: simply_core::storage::ids::UserId,

    // Individual services
    mcp: Arc<McpService>,
    model: Arc<ModelService>,
    asset: Arc<AssetService<S>>,
    voice: Arc<VoiceService>,
    daemon_info: Arc<DaemonInfoService>,
}

impl<S: StorageTypes> EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    pub async fn new<T: Stores<S> + 'static>(
        stores: Arc<T>,
    ) -> anyhow::Result<Arc<Self>>
    where
        S::User: simply_core::storage::traits::UserStore,
    {
        let coordinator = Arc::new(StorageCoordinator::from_stores(&*stores));
        let stores: Arc<dyn Stores<S>> = stores;
        let settings = config::Settings::load();

        let user_id = Self::resolve_user(&*stores, &settings).await?;

        const FALLBACK_MODEL_ID: &str = "claude/models/claude-sonnet-4-5-20250929";
        let default_model_id = settings
            .default_model
            .unwrap_or_else(|| FALLBACK_MODEL_ID.to_string());

        let document_resolver: Arc<dyn DocumentResolver> = stores.document();
        let mcp = Arc::new(McpService::start(
            McpServiceConfig {
                oauth_callback_port: settings.oauth_callback_port,
            },
        )
        .await?);

        let model = Arc::new(ModelService::new(default_model_id));
        let asset = Arc::new(AssetService::new(Arc::clone(&coordinator), Arc::clone(&stores)));
        let voice = Arc::new(VoiceService);
        let daemon_info = Arc::new(DaemonInfoService);

        let daemon = Arc::new(Self {
            coordinator,
            stores,
            sessions: Mutex::new(HashMap::new()),
            user_id,
            mcp,
            model,
            asset,
            voice,
            daemon_info,
        });

        Self::spawn_session_reaper(Arc::clone(&daemon));

        Ok(daemon)
    }

    // -- Service accessors for main.rs / RestDispatcher registration ----------

    pub fn mcp_service(&self) -> Arc<McpService> { Arc::clone(&self.mcp) }
    pub fn model_service(&self) -> Arc<ModelService> { Arc::clone(&self.model) }
    pub fn asset_service(&self) -> Arc<AssetService<S>> { Arc::clone(&self.asset) }
    pub fn voice_service(&self) -> Arc<VoiceService> { Arc::clone(&self.voice) }
    pub fn daemon_info_service(&self) -> Arc<DaemonInfoService> { Arc::clone(&self.daemon_info) }

    pub fn oauth_redirect_uri(&self) -> String { self.mcp.oauth_redirect_uri() }
    pub fn stores(&self) -> &Arc<dyn Stores<S>> { &self.stores }
    pub fn coordinator(&self) -> &Arc<StorageCoordinator<S>> { &self.coordinator }

    // -- Internal helpers -----------------------------------------------------

    fn spawn_session_reaper(daemon: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let mut sessions = daemon.sessions.lock().await;
                let before = sessions.len();
                sessions.retain(|sid, managed| {
                    if managed.event_broadcast.receiver_count() == 0 {
                        tracing::info!(session_id = %sid, "reaping orphaned session");
                        false
                    } else {
                        true
                    }
                });
                let reaped = before - sessions.len();
                if reaped > 0 {
                    tracing::info!(reaped, remaining = sessions.len(), "session reaper sweep");
                }
            }
        });
    }

    fn spawn_event_forwarder(
        session_id: &SessionId,
        broadcast_tx: &broadcast::Sender<DaemonEvent>,
    ) -> SessionEventSender {
        let (evt_tx, mut evt_rx) = mpsc::unbounded_channel::<(String, SessionEvent)>();
        let broadcast_tx = broadcast_tx.clone();
        let sid = session_id.clone();

        tokio::spawn(async move {
            while let Some((_key, event)) = evt_rx.recv().await {
                for de in Self::session_event_to_daemon_events(event) {
                    let _ = broadcast_tx.send(de);
                }
            }
            tracing::debug!(session_id = %sid, "event forwarder stopped");
        });

        evt_tx
    }

    fn session_event_to_daemon_events(event: SessionEvent) -> Vec<DaemonEvent> {
        match event {
            SessionEvent::UserMessageAdded(msg) => vec![DaemonEvent::UserMessage(msg)],
            SessionEvent::TextDelta(text) => vec![DaemonEvent::TextDelta(text)],
            SessionEvent::ContentBlock(block) => vec![DaemonEvent::AssistantContent(block)],
            SessionEvent::ToolCallStart { id, name, arguments } => {
                vec![DaemonEvent::ToolCall { id, name, arguments }]
            }
            SessionEvent::ToolCallResult { id, content } => {
                let result = serde_json::to_value(&content).unwrap_or_default();
                vec![DaemonEvent::ToolResult { id, result }]
            }
            SessionEvent::AssistantMessage(msg) => {
                msg.payload.content.into_iter().map(DaemonEvent::AssistantContent).collect()
            }
            SessionEvent::TurnComplete { .. } => vec![DaemonEvent::TurnComplete],
            SessionEvent::Error(err) => vec![DaemonEvent::Error(err)],
            SessionEvent::ModelChanged(_) => vec![],
        }
    }

    fn convert_seed(seed: Vec<SeedMessage>) -> Vec<llm::ChatMessage> {
        seed.into_iter()
            .map(|sm| {
                let blocks: Vec<llm::ContentBlock> = sm
                    .content
                    .into_iter()
                    .filter_map(|c| c.into_content_block_inline())
                    .collect();
                llm::ChatMessage::new(sm.role, llm::ChatPayload::new(blocks))
            })
            .collect()
    }

    fn convert_input_inline(content: Vec<simply_core::storage::content::InputContent>) -> Vec<llm::ContentBlock> {
        content
            .into_iter()
            .filter_map(|c| c.into_content_block_inline())
            .collect()
    }

    async fn resolve_user(
        stores: &dyn Stores<S>,
        settings: &config::Settings,
    ) -> anyhow::Result<simply_core::storage::ids::UserId>
    where
        S::User: simply_core::storage::traits::UserStore,
    {
        use simply_core::storage::traits::UserStore;
        let user_store = stores.user();

        let user = if let Some(ref email) = settings.user_email {
            user_store.get_or_create_user_by_email(email).await?
        } else {
            let users = user_store.list_users().await?;
            match users.len() {
                0 => user_store.get_or_create_default_user().await?,
                1 => users.into_iter().next().unwrap(),
                _ => {
                    let emails: Vec<String> = users.iter().map(|u| u.email.clone()).collect();
                    anyhow::bail!("MULTIPLE_USERS:{}", emails.join(","));
                }
            }
        };
        Ok(user.id)
    }

    fn make_session_info(session_id: &SessionId, persistence: Persistence, model_id: String) -> SessionInfo {
        SessionInfo {
            id: session_id.clone(),
            persistence,
            model_id,
            created_at: {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                format!("{}", now.as_secs())
            },
        }
    }
}

// ---------------------------------------------------------------------------
// SessionApi — implemented directly (owns session state)
// ---------------------------------------------------------------------------

#[async_trait]
impl<S: StorageTypes> SessionApi for EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    async fn create_session(
        &self,
        options: CreateSessionOptions,
    ) -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)> {
        let persistence = options.persistence.clone().unwrap_or(Persistence::Ephemeral);

        // Reuse existing session for the same persistent conversation.
        if let Persistence::Persistent { ref conversation_id } = persistence {
            let sessions = self.sessions.lock().await;
            if let Some(managed) = sessions.values().find(|m| matches!(
                &m.info.persistence,
                Persistence::Persistent { conversation_id: cid } if cid == conversation_id
            )) {
                return Ok((managed.info.clone(), managed.event_broadcast.subscribe()));
            }
        }

        let session_id = SessionId::generate();
        let model_id = match options.model_id {
            Some(id) => id,
            None => self.model.default_model().await,
        };
        let model = llm::create_model(&model_id)?;
        let seed_messages = Self::convert_seed(options.seed);
        let (broadcast_tx, broadcast_rx) = broadcast::channel(256);
        let info = Self::make_session_info(&session_id, persistence.clone(), model_id);

        let tools: Arc<dyn ToolService> =
            Arc::new(McpToolRegistry::new(Arc::clone(self.mcp.registry())));
        let document_resolver: Arc<dyn DocumentResolver> = self.stores.document();
        let evt_tx = Self::spawn_event_forwarder(&session_id, &broadcast_tx);

        let manager = SessionManager::create(
            session_id.as_str().to_string(),
            persistence,
            seed_messages,
            options.system_prompt,
            model,
            tools,
            Arc::clone(&self.coordinator),
            document_resolver,
            simply_core::ExecutionContext::default(),
            evt_tx,
        ).await?;

        self.sessions.lock().await.insert(session_id.clone(), ManagedSession {
            info: info.clone(),
            manager,
            event_broadcast: broadcast_tx,
        });

        tracing::info!(session_id = %session_id, "session created");
        Ok((info, broadcast_rx))
    }

    async fn subscribe_session(&self, session_id: &SessionId) -> anyhow::Result<broadcast::Receiver<DaemonEvent>> {
        let sessions = self.sessions.lock().await;
        let managed = sessions.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        Ok(managed.event_broadcast.subscribe())
    }

    async fn close_session(&self, session_id: &SessionId) -> anyhow::Result<()> {
        if self.sessions.lock().await.remove(session_id).is_some() {
            tracing::info!(session_id = %session_id, "session closed");
        }
        Ok(())
    }

    async fn close_all_sessions(&self) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().await;
        let count = sessions.len();
        sessions.clear();
        tracing::info!(count, "all sessions closed");
        Ok(())
    }

    async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>> {
        Ok(self.sessions.lock().await.values().map(|s| s.info.clone()).collect())
    }

    async fn send_message(&self, session_id: &SessionId, message: UserMessage) -> anyhow::Result<()> {
        let sessions = self.sessions.lock().await;
        let managed = sessions.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        managed.manager.send_message(Self::convert_input_inline(message.content));
        Ok(())
    }

    async fn set_model(&self, session_id: &SessionId, model_id: &str) -> anyhow::Result<()> {
        let new_model = llm::create_model(model_id)?;
        let mut sessions = self.sessions.lock().await;
        let managed = sessions.get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        managed.manager.set_model(new_model);
        managed.info.model_id = model_id.to_string();
        Ok(())
    }

    async fn push_event(&self, event: InboundEvent) -> anyhow::Result<()> {
        tracing::info!(event_type = %event.event_type, "inbound event received");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ConversationApi — implemented directly (needs session state for delete)
// ---------------------------------------------------------------------------

#[async_trait]
impl<S: StorageTypes> ConversationApi for EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    async fn create_conversation(&self, name: Option<String>) -> anyhow::Result<ConversationId> {
        self.coordinator.create_conversation(&self.user_id, name.as_deref()).await
    }

    async fn list_conversations(&self) -> anyhow::Result<Vec<ConversationInfo>> {
        use simply_core::storage::{EntityStore, EntityType, TurnStore};
        let entities = self.stores.entity()
            .list_entities(&self.user_id, Some(&EntityType::conversation())).await?;
        let mut result = Vec::with_capacity(entities.len());
        for entity in entities {
            let turn_count = self.stores.turn().get_turn_count(&entity.id).await.unwrap_or(0);
            result.push(ConversationInfo {
                id: entity.id.clone(),
                name: entity.name.clone(),
                message_count: turn_count,
                created_at: entity.created_at,
            });
        }
        Ok(result)
    }

    async fn delete_conversation(&self, conversation_id: &ConversationId) -> anyhow::Result<()> {
        use simply_core::storage::EntityStore;
        let session_id = {
            let sessions = self.sessions.lock().await;
            sessions.iter()
                .find(|(_, m)| matches!(
                    &m.info.persistence,
                    Persistence::Persistent { conversation_id: cid } if cid == conversation_id
                ))
                .map(|(sid, _)| sid.clone())
        };
        if let Some(sid) = session_id {
            let _ = self.close_session(&sid).await;
        }
        self.stores.entity().delete_entity(conversation_id).await
    }

    async fn rename_conversation(&self, conversation_id: &ConversationId, name: &str) -> anyhow::Result<()> {
        use simply_core::storage::EntityStore;
        let mut entity = self.stores.entity().get_entity(conversation_id).await?
            .ok_or_else(|| anyhow::anyhow!("conversation not found: {conversation_id}"))?;
        entity.name = if name.trim().is_empty() { None } else { Some(name.to_string()) };
        self.stores.entity().update_entity(conversation_id, &entity).await
    }

    async fn get_messages(&self, conversation_id: &ConversationId) -> anyhow::Result<Vec<ResolvedMessage>> {
        self.coordinator.open_session(conversation_id).await
    }
}

// ---------------------------------------------------------------------------
// Delegated traits — forward to inner services for DaemonApi compat
// ---------------------------------------------------------------------------

#[async_trait]
impl<S: StorageTypes> McpApi for EmbeddedDaemon<S>
where S::Document: DocumentResolver,
{
    async fn list_mcp_servers(&self) -> anyhow::Result<Vec<McpServerInfo>> { self.mcp.list_mcp_servers().await }
    async fn add_mcp_server(&self, request: AddMcpServerRequest) -> anyhow::Result<()> { self.mcp.add_mcp_server(request).await }
    async fn remove_mcp_server(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.remove_mcp_server(server_id).await }
    async fn connect_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> { self.mcp.connect_mcp_server(server_id).await }
    async fn disconnect_mcp_server(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.disconnect_mcp_server(server_id).await }
    async fn get_mcp_server_tools(&self, server_id: &str) -> anyhow::Result<Vec<McpTool>> { self.mcp.get_mcp_server_tools(server_id).await }
    async fn test_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> { self.mcp.test_mcp_server(server_id).await }
    async fn update_mcp_server_settings(&self, server_id: &str, request: UpdateMcpServerRequest) -> anyhow::Result<()> { self.mcp.update_mcp_server_settings(server_id, request).await }
    async fn stop_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.stop_mcp_retry(server_id).await }
    async fn start_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.start_mcp_retry(server_id).await }
    async fn register_ephemeral_mcp(&self, request: RegisterEphemeralRequest) -> anyhow::Result<usize> { self.mcp.register_ephemeral_mcp(request).await }
    async fn unregister_ephemeral_mcp(&self, server_id: &str) -> anyhow::Result<()> { self.mcp.unregister_ephemeral_mcp(server_id).await }
    async fn list_all_tools(&self) -> anyhow::Result<Vec<McpTool>> { self.mcp.list_all_tools().await }
    async fn call_tool_direct(&self, request: CallToolRequestParam) -> anyhow::Result<CallToolResult> { self.mcp.call_tool_direct(request).await }
}

#[async_trait]
impl<S: StorageTypes> OAuthApi for EmbeddedDaemon<S>
where S::Document: DocumentResolver,
{
    async fn start_oauth(&self, server_id: &str) -> anyhow::Result<OAuthFlowInfo> { self.mcp.start_oauth(server_id).await }
    async fn complete_oauth(&self, server_id: &str, code: &str, state: &str) -> anyhow::Result<()> { self.mcp.complete_oauth(server_id, code, state).await }
    async fn complete_oauth_with_code(&self, server_id: &str, code: &str) -> anyhow::Result<()> { self.mcp.complete_oauth_with_code(server_id, code).await }
    async fn resolve_oauth_state(&self, state: &str) -> Option<String> { self.mcp.resolve_oauth_state(state).await }
}

#[async_trait]
impl<S: StorageTypes> ModelApi for EmbeddedDaemon<S>
where S::Document: DocumentResolver,
{
    async fn list_models(&self) -> anyhow::Result<Vec<llm::ModelInfo>> { self.model.list_models().await }
    async fn list_providers(&self) -> Vec<llm::ProviderInfo> { self.model.list_providers().await }
    async fn default_model_id(&self) -> String { self.model.default_model_id().await }
    async fn set_default_model(&self, model_id: &str) -> anyhow::Result<()> { self.model.set_default_model(model_id).await }
}

#[async_trait]
impl<S: StorageTypes> AssetApi for EmbeddedDaemon<S>
where S::Document: DocumentResolver,
{
    async fn store_asset(&self, upload: simply_rpc::BinaryUpload) -> anyhow::Result<AssetInfo> { self.asset.store_asset(upload).await }
    async fn list_assets(&self) -> anyhow::Result<Vec<AssetId>> { self.asset.list_assets().await }
    async fn get_asset(&self, id: &AssetId) -> anyhow::Result<AssetInfo> { self.asset.get_asset(id).await }
    async fn get_blob(&self, hash: &simply_core::storage::types::BlobHash) -> anyhow::Result<simply_rpc::BinaryResponse> { self.asset.get_blob(hash).await }
}

#[async_trait]
impl<S: StorageTypes> VoiceApi for EmbeddedDaemon<S>
where S::Document: DocumentResolver,
{
    async fn voice_connect(&self, session_id: &SessionId) -> anyhow::Result<VoiceHandle> { self.voice.voice_connect(session_id).await }
    async fn voice_disconnect(&self, session_id: &SessionId) -> anyhow::Result<()> { self.voice.voice_disconnect(session_id).await }
}

#[async_trait]
impl<S: StorageTypes> DaemonInfoApi for EmbeddedDaemon<S>
where S::Document: DocumentResolver,
{
    async fn health(&self) -> anyhow::Result<DaemonHealth> { self.daemon_info.health().await }
    async fn kill(&self) -> anyhow::Result<()> { self.daemon_info.kill().await }
    async fn version(&self) -> anyhow::Result<String> { self.daemon_info.version().await }
}
