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

use simply_rpc::RpcService;

use crate::api::*;
use crate::mcp::{McpService, McpServiceConfig};
use crate::services::*;
use crate::tools::{CompositeToolService, DaemonToolService};

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
    // Individual services
    mcp: Arc<McpService>,
    model: Arc<ModelService>,
    asset: Arc<AssetService<S>>,
    document: Arc<DocumentService<S>>,
    voice: Arc<VoiceService>,
    core: Arc<CoreService>,
    search: Arc<SearchService<S>>,
    user_svc: Arc<UserService<S>>,
    tools: Arc<CompositeToolService>,
    user_tools: Arc<crate::user_tools::UserToolServiceCache>,
}

impl<S: StorageTypes> EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    pub async fn new<T: Stores<S> + 'static>(
        stores: Arc<T>,
        vector_store: Arc<dyn simply_core::embedding::VectorStore>,
        token_store: Arc<crate::token_store::TransientTokenStore>,
    ) -> anyhow::Result<Arc<Self>>
    where
        S::User: simply_core::storage::traits::UserStore,
    {
        let coordinator = Arc::new(StorageCoordinator::from_stores(&*stores));
        let stores: Arc<dyn Stores<S>> = stores;
        let settings = config::Settings::load();

        const FALLBACK_MODEL_ID: &str = "claude/models/claude-sonnet-4-5-20250929";
        let default_model_id = settings
            .default_model.clone()
            .unwrap_or_else(|| FALLBACK_MODEL_ID.to_string());

        let document_resolver: Arc<dyn DocumentResolver> = stores.document();
        let daemon_port = settings.daemon_port.unwrap_or(config::DEFAULT_DAEMON_PORT);
        let public_url = settings.public_url.clone()
            .unwrap_or_else(|| format!("http://localhost:{}", daemon_port));
        let mcp = Arc::new(McpService::start(
            McpServiceConfig {
                public_url,
            },
        )
        .await?);

        let model = Arc::new(ModelService::new(default_model_id));
        let asset = Arc::new(AssetService::new(Arc::clone(&coordinator), Arc::clone(&stores)));
        let mut voice_service = VoiceService::new();

        // Register Whisper (local STT) — only if compiled with whisper feature
        #[cfg(feature = "whisper")]
        if let Some(path) = config::PathManager::whisper_model_path() {
            if path.exists() {
                match simply_voice::WhisperProvider::new(&path) {
                    Ok(p) => {
                        tracing::info!("whisper STT loaded from {}", path.display());
                        voice_service = voice_service.register_stt(
                            "whisper", "Whisper (local)", Arc::new(p),
                        );
                    }
                    Err(e) => tracing::warn!("failed to load whisper model: {e}"),
                }
            }
        }

        // Register Voxtral — local MLX server or Mistral API
        if let Ok(base_url) = std::env::var("VOXTRAL_BASE_URL") {
            let voxtral = Arc::new(simply_voice::VoxtralProvider::local(&base_url));
            voice_service = voice_service
                .register_stt("voxtral", "Voxtral (local)", voxtral.clone())
                .register_tts("voxtral", "Voxtral (local)", voxtral);
            tracing::info!(base_url = %base_url, "voxtral local STT + TTS registered");
        } else if let Some(key) = settings.get_api_key("mistral")
            .or_else(|| std::env::var("MISTRAL_API_KEY").ok()) {
            let voxtral = Arc::new(simply_voice::VoxtralProvider::new(key));
            voice_service = voice_service
                .register_stt("voxtral", "Voxtral (Mistral)", voxtral.clone())
                .register_tts("voxtral", "Voxtral (Mistral)", voxtral);
            tracing::info!("voxtral STT + TTS registered (Mistral API)");
        }

        // Register ElevenLabs (STT + TTS)
        if let Some(key) = settings.get_api_key("elevenlabs")
            .or_else(|| std::env::var("ELEVENLABS_API_KEY").ok()) {
            let elevenlabs = Arc::new(simply_voice::ElevenLabsProvider::new(key));
            voice_service = voice_service
                .register_stt("elevenlabs", "ElevenLabs", elevenlabs.clone())
                .register_tts("elevenlabs", "ElevenLabs", elevenlabs);
            tracing::info!("elevenlabs STT + TTS registered");
        }

        // Register Gemini Realtime
        if let Some(key) = settings.get_api_key("google")
            .or_else(|| std::env::var("GOOGLE_API_KEY").ok()) {
            let gemini = Arc::new(simply_voice::GeminiRealtimeProvider::new(key));
            voice_service = voice_service.register_realtime(
                "gemini", "Gemini Realtime", gemini,
            );
            tracing::info!("gemini realtime registered");
        }

        let voice = Arc::new(voice_service);

        // -- Embedding / RAG stack -----------------------------------------------
        let embedding_config = &settings.embedding;
        let embedding_provider: Arc<dyn llm::EmbeddingProvider> = Self::create_embedding_provider(
            &embedding_config, &settings,
        )?;
        let chunker: Arc<dyn simply_core::embedding::Chunker> = Arc::new(
            simply_core::embedding::RecursiveCharacterChunker::new(
                embedding_config.chunk_size,
                embedding_config.chunk_overlap,
            ),
        );
        let vector_store: Arc<dyn simply_core::embedding::VectorStore> = vector_store;
        let embedding_queue = Arc::new(
            crate::embedding_queue::ChannelEmbeddingQueue::new(
                Arc::clone(&embedding_provider),
                Arc::clone(&chunker),
                Arc::clone(&vector_store),
            ),
        );
        tracing::info!(
            provider = embedding_config.provider.as_str(),
            model = embedding_config.model.as_str(),
            "embedding provider initialized"
        );

        let document = Arc::new(
            DocumentService::new(Arc::clone(&stores))
                .with_embedding(
                    embedding_queue.clone() as Arc<dyn crate::embedding_queue::EmbeddingQueue>,
                    Arc::clone(&vector_store),
                )
        );
        let core = Arc::new(CoreService::embedded());

        let user_svc = Arc::new(UserService::new(Arc::clone(&stores)));

        let search = Arc::new(SearchService::new(
            embedding_provider,
            vector_store,
            embedding_queue,
            Arc::clone(&stores),
        ));

        let daemon_tools = Arc::new(
            DaemonToolService::new()
                .register(<dyn AssetApi>::service(asset.clone()))
                .register(<dyn DocumentApi>::service(document.clone()))
                .register(<dyn ModelApi>::service(model.clone()))
                .register(<dyn CoreApi>::service(core.clone()))
                .register(<dyn McpApi>::service(mcp.clone()))
                .register(<dyn VoiceApi>::service(voice.clone()))
                .register(<dyn SearchApi>::service(search.clone())),
        );

        // Per-user tool service cache — resolves MCP tools based on user's OAuth tokens
        let user_tools = Arc::new(crate::user_tools::UserToolServiceCache::new(
            daemon_tools,
            token_store,
            Arc::clone(mcp.registry()),
        ));

        let tools = Arc::new(CompositeToolService::new(
            DaemonToolService::new()
                .register(<dyn AssetApi>::service(asset.clone()))
                .register(<dyn DocumentApi>::service(document.clone()))
                .register(<dyn ModelApi>::service(model.clone()))
                .register(<dyn CoreApi>::service(core.clone()))
                .register(<dyn McpApi>::service(mcp.clone()))
                .register(<dyn VoiceApi>::service(voice.clone()))
                .register(<dyn SearchApi>::service(search.clone())),
            McpToolRegistry::new(Arc::clone(mcp.registry())),
            Arc::clone(&mcp),
            Arc::clone(&user_tools),
        ));

        let daemon = Arc::new(Self {
            coordinator,
            stores,
            sessions: Mutex::new(HashMap::new()),
            mcp,
            model,
            asset,
            document,
            voice,
            core,
            search,
            user_svc,
            tools,
            user_tools,
        });

        Self::spawn_session_reaper(Arc::clone(&daemon));

        Ok(daemon)
    }

    // -- Service accessors for main.rs / ServiceRouter registration ----------

    pub fn mcp_service(&self) -> Arc<dyn McpApi> { self.tools.clone() }
    pub fn oauth_service(&self) -> Arc<dyn OAuthApi> { self.mcp.clone() }
    pub fn model_service(&self) -> Arc<ModelService> { Arc::clone(&self.model) }
    pub fn asset_service(&self) -> Arc<AssetService<S>> { Arc::clone(&self.asset) }
    pub fn document_service(&self) -> Arc<DocumentService<S>> { Arc::clone(&self.document) }
    pub fn voice_service(&self) -> Arc<VoiceService> { Arc::clone(&self.voice) }
    pub fn core_service(&self) -> Arc<CoreService> { Arc::clone(&self.core) }
    pub fn search_service(&self) -> Arc<SearchService<S>> { Arc::clone(&self.search) }
    pub fn user_service(&self) -> Arc<UserService<S>> { Arc::clone(&self.user_svc) }

    pub fn oauth_redirect_uri(&self) -> String { self.mcp.oauth_redirect_uri() }
    pub fn oauth_tracker(&self) -> Arc<crate::oauth::callback::CallbackTracker> { self.mcp.oauth_tracker() }
    pub fn stores(&self) -> &Arc<dyn Stores<S>> { &self.stores }
    pub fn coordinator(&self) -> &Arc<StorageCoordinator<S>> { &self.coordinator }

    // -- Internal helpers -----------------------------------------------------

    fn create_embedding_provider(
        embedding_config: &config::EmbeddingConfig,
        settings: &config::Settings,
    ) -> anyhow::Result<Arc<dyn llm::EmbeddingProvider>> {
        match embedding_config.provider.as_str() {
            "local" => {
                let provider = llm::providers::LocalEmbeddingProvider::new(&embedding_config.model)?;
                Ok(Arc::new(provider))
            }
            "ollama" => {
                let provider = match std::env::var("OLLAMA_BASE_URL") {
                    Ok(base_url) => llm::providers::OllamaProvider::new(&base_url),
                    Err(_) => llm::providers::OllamaProvider::default(),
                };
                Ok(Arc::new(provider.create_embedding_provider(&embedding_config.model)))
            }
            "mistral" => {
                let api_key = settings.get_api_key("mistral")
                    .or_else(|| std::env::var("MISTRAL_API_KEY").ok())
                    .ok_or_else(|| anyhow::anyhow!("mistral API key not configured"))?;
                let provider = llm::providers::MistralProvider::default(&api_key);
                Ok(Arc::new(provider.create_embedding_provider(&embedding_config.model)))
            }
            "gemini" => {
                let api_key = settings.get_api_key("google")
                    .or_else(|| std::env::var("GOOGLE_API_KEY").ok())
                    .or_else(|| settings.get_api_key("gemini"))
                    .or_else(|| std::env::var("GEMINI_API_KEY").ok())
                    .ok_or_else(|| anyhow::anyhow!("google/gemini API key not configured"))?;
                let provider = llm::providers::GeminiProvider::default(&api_key);
                Ok(Arc::new(provider.create_embedding_provider(&embedding_config.model)))
            }
            "claude" | "voyage" => {
                let api_key = settings.get_api_key("voyage")
                    .or_else(|| std::env::var("VOYAGE_API_KEY").ok())
                    .ok_or_else(|| anyhow::anyhow!("voyage API key not configured (Claude embeddings use Voyage AI)"))?;
                Ok(Arc::new(llm::providers::VoyageEmbeddingProvider::new(&api_key, embedding_config.model.clone())))
            }
            other => anyhow::bail!("unknown embedding provider: {other}"),
        }
    }

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

    fn require_user(&self, ctx: &simply_rpc::RequestContext) -> anyhow::Result<simply_core::storage::ids::UserId> {
        ctx.scope.user_id.as_ref()
            .map(|id| simply_core::storage::ids::UserId::from_string(id))
            .ok_or_else(|| anyhow::anyhow!("authentication required"))
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
        ctx: &simply_rpc::RequestContext,
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

        // Resolve user-scoped tools from request context
        let tools: Arc<dyn ToolService> = match &ctx.scope.user_id {
            Some(uid) => match self.user_tools.get(&simply_core::storage::ids::UserId::from_string(uid)).await {
                Ok(user_tools) => user_tools,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to build user tools, falling back to global");
                    self.tools.clone()
                }
            },
            None => self.tools.clone(),
        };
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

    async fn subscribe_session(&self, _ctx: &simply_rpc::RequestContext, session_id: &SessionId) -> anyhow::Result<broadcast::Receiver<DaemonEvent>> {
        let sessions = self.sessions.lock().await;
        let managed = sessions.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        Ok(managed.event_broadcast.subscribe())
    }

    async fn close_session(&self, _ctx: &simply_rpc::RequestContext, session_id: &SessionId) -> anyhow::Result<()> {
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

    async fn list_sessions(&self, _ctx: &simply_rpc::RequestContext) -> anyhow::Result<Vec<SessionInfo>> {
        Ok(self.sessions.lock().await.values().map(|s| s.info.clone()).collect())
    }

    async fn send_message(&self, _ctx: &simply_rpc::RequestContext, session_id: &SessionId, message: UserMessage) -> anyhow::Result<()> {
        let sessions = self.sessions.lock().await;
        let managed = sessions.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        managed.manager.send_message(Self::convert_input_inline(message.content));
        Ok(())
    }

    async fn set_model(&self, _ctx: &simply_rpc::RequestContext, session_id: &SessionId, model_id: &str) -> anyhow::Result<()> {
        let new_model = llm::create_model(model_id)?;
        let mut sessions = self.sessions.lock().await;
        let managed = sessions.get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        managed.manager.set_model(new_model);
        managed.info.model_id = model_id.to_string();
        Ok(())
    }

    async fn push_event(&self, _ctx: &simply_rpc::RequestContext, event: InboundEvent) -> anyhow::Result<()> {
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
    async fn create_conversation(&self, ctx: &simply_rpc::RequestContext, name: Option<String>) -> anyhow::Result<ConversationId> {
        let user_id = self.require_user(ctx)?;
        self.coordinator.create_conversation(&user_id, name.as_deref()).await
    }

    async fn list_conversations(&self, ctx: &simply_rpc::RequestContext) -> anyhow::Result<Vec<ConversationInfo>> {
        use simply_core::storage::{EntityStore, EntityType, TurnStore};
        let user_id = self.require_user(ctx)?;
        let entities = self.stores.entity()
            .list_entities(&user_id, Some(&EntityType::conversation())).await?;
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

    async fn delete_conversation(&self, ctx: &simply_rpc::RequestContext, conversation_id: &ConversationId) -> anyhow::Result<()> {
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
            let _ = self.close_session(ctx, &sid).await;
        }
        self.stores.entity().delete_entity(conversation_id).await
    }

    async fn rename_conversation(&self, _ctx: &simply_rpc::RequestContext, conversation_id: &ConversationId, name: &str) -> anyhow::Result<()> {
        use simply_core::storage::EntityStore;
        let mut entity = self.stores.entity().get_entity(conversation_id).await?
            .ok_or_else(|| anyhow::anyhow!("conversation not found: {conversation_id}"))?;
        entity.name = if name.trim().is_empty() { None } else { Some(name.to_string()) };
        self.stores.entity().update_entity(conversation_id, &entity).await
    }

    async fn get_messages(&self, _ctx: &simply_rpc::RequestContext, conversation_id: &ConversationId) -> anyhow::Result<Vec<ResolvedMessage>> {
        self.coordinator.open_session(conversation_id).await
    }
}

// ---------------------------------------------------------------------------
// Daemon trait — service bag with direct delegation
// ---------------------------------------------------------------------------

impl<S: StorageTypes> Daemon for EmbeddedDaemon<S>
where S::Document: DocumentResolver,
{
    fn session(&self) -> &dyn SessionApi { self }
    fn conversation(&self) -> &dyn ConversationApi { self }
    fn document(&self) -> &dyn DocumentApi { &*self.document }
    fn mcp(&self) -> &dyn McpApi { &*self.tools }
    fn oauth(&self) -> &dyn OAuthApi { &*self.mcp }
    fn model(&self) -> &dyn ModelApi { &*self.model }
    fn asset(&self) -> &dyn AssetApi { &*self.asset }
    fn voice(&self) -> &dyn VoiceApi { &*self.voice }
    fn core(&self) -> &dyn CoreApi { &*self.core }
    fn search(&self) -> &dyn SearchApi { &*self.search }
    fn user(&self) -> &dyn UserApi { &*self.user_svc }
    fn tools(&self) -> &dyn ToolService { &*self.tools }
}
