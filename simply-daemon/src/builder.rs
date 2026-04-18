//! Daemon builder — constructs services and assembles the EmbeddedDaemon.
//!
//! This module provides `build_daemon()` which is the single entry point for
//! constructing a fully wired EmbeddedDaemon. Used by:
//! - `simply-daemon/src/main.rs` (standalone daemon)
//! - `lumina/src/main.rs` (embedded in Discord bot)
//! - Any future host that needs an in-process daemon

use std::sync::Arc;

use simply_core::storage::traits::{StorageTypes, Stores};
use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::DocumentResolver;
use simply_core::McpToolRegistry;
use simply_rpc::{ServiceRouter, RpcService};

use crate::api::*;
use crate::embedded::EmbeddedDaemon;
use crate::embedding_queue::{ChannelEmbeddingQueue, EmbeddingQueue};
use crate::mcp::{McpService, McpServiceConfig};
use crate::services::*;
use crate::tools::{CompositeToolService, DaemonToolService};

/// Configuration for building an EmbeddedDaemon.
pub struct DaemonBuilder<S: StorageTypes> {
    pub stores: Arc<dyn Stores<S>>,
    pub coordinator: Arc<StorageCoordinator<S>>,
    pub vector_store: Arc<dyn simply_core::embedding::VectorStore>,
    pub token_store: Arc<crate::token_store::TransientTokenStore>,
    pub voice: VoiceService,
    pub skills: Vec<Arc<dyn simply_daemon_api::Skill>>,
}

impl<S: StorageTypes> DaemonBuilder<S>
where
    S::Document: DocumentResolver,
    S::User: simply_core::storage::traits::UserStore,
{
    /// Build and return a fully wired EmbeddedDaemon.
    pub async fn build(self) -> anyhow::Result<Arc<EmbeddedDaemon<S>>> {
        let settings = config::Settings::load();

        const FALLBACK_MODEL_ID: &str = "claude/models/claude-sonnet-4-5-20250929";
        let default_model_id = settings.default_model.clone()
            .unwrap_or_else(|| FALLBACK_MODEL_ID.to_string());

        let daemon_port = settings.daemon_port.unwrap_or(config::DEFAULT_DAEMON_PORT);
        let public_url = settings.public_url.clone()
            .unwrap_or_else(|| format!("http://localhost:{}", daemon_port));

        let mcp = Arc::new(McpService::start(
            McpServiceConfig { public_url },
        ).await?);

        let model = Arc::new(ModelService::new(default_model_id));
        let asset = Arc::new(AssetService::new(
            Arc::clone(&self.coordinator),
            Arc::clone(&self.stores),
        ));
        let voice = Arc::new(self.voice);

        // Embedding / RAG
        let embedding_config = &settings.embedding;
        let embedding_provider = create_embedding_provider(embedding_config, &settings)?;
        let chunker: Arc<dyn simply_core::embedding::Chunker> = Arc::new(
            simply_core::embedding::RecursiveCharacterChunker::new(
                embedding_config.chunk_size,
                embedding_config.chunk_overlap,
            ),
        );
        let embedding_queue = Arc::new(ChannelEmbeddingQueue::new(
            Arc::clone(&embedding_provider),
            Arc::clone(&chunker),
            Arc::clone(&self.vector_store),
        ));
        tracing::info!(
            provider = embedding_config.provider.as_str(),
            model = embedding_config.model.as_str(),
            "embedding provider initialized"
        );

        let document = Arc::new(
            DocumentService::new(Arc::clone(&self.stores))
                .with_embedding(
                    embedding_queue.clone() as Arc<dyn EmbeddingQueue>,
                    Arc::clone(&self.vector_store),
                )
        );
        let core = Arc::new(CoreService::embedded());
        let user_svc = Arc::new(UserService::new(Arc::clone(&self.stores)));
        let search = Arc::new(SearchService::new(
            embedding_provider,
            self.vector_store,
            embedding_queue,
            Arc::clone(&self.stores),
        ));

        // Tool service — built once, shared between user_tools and composite
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

        let user_tools = Arc::new(crate::user_tools::UserToolServiceCache::new(
            Arc::clone(&daemon_tools),
            self.token_store,
            Arc::clone(mcp.registry()),
        ));

        let mut composite = CompositeToolService::new(
            daemon_tools,
            McpToolRegistry::new(Arc::clone(mcp.registry())),
            Arc::clone(&mcp),
            Arc::clone(&user_tools),
        );
        for skill in self.skills {
            composite = composite.register_skill(skill);
        }
        let tools = Arc::new(composite);

        EmbeddedDaemon::assemble(
            self.coordinator,
            self.stores,
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
        )
    }
}

/// Build the ServiceRouter from a daemon.
///
/// Reuses the daemon_tools service list (registered once in the builder)
/// and adds SessionApi + ConversationApi (implemented directly by the daemon).
/// Optionally adds a CoreApi (with kill channel) provided by the caller.
pub fn build_service_router<S: StorageTypes>(
    daemon: &Arc<EmbeddedDaemon<S>>,
    extra_services: Vec<Arc<dyn simply_rpc::RestService>>,
) -> Arc<ServiceRouter>
where
    S::Document: DocumentResolver,
{
    let session_svc: Arc<dyn SessionApi> = daemon.clone();
    let conversation_svc: Arc<dyn ConversationApi> = daemon.clone();

    let mut router = ServiceRouter::new()
        .register(<dyn SessionApi>::service(session_svc))
        .register(<dyn ConversationApi>::service(conversation_svc));

    // Register all daemon tool services (Asset, Document, MCP, Model, Voice, Search, etc.)
    for svc in daemon.daemon_tool_services() {
        router = router.register(svc);
    }

    // Extra services (e.g., CoreApi with kill channel)
    for svc in extra_services {
        router = router.register(svc);
    }

    Arc::new(router)
}

/// Create voice service with all configured providers.
pub fn create_voice_service() -> VoiceService {
    let settings = config::Settings::load();
    let mut voice = VoiceService::new();

    #[cfg(feature = "whisper")]
    if let Some(path) = config::PathManager::whisper_model_path() {
        if path.exists() {
            match simply_voice::WhisperProvider::new(&path) {
                Ok(p) => {
                    tracing::info!("whisper STT loaded from {}", path.display());
                    voice = voice.register_stt("whisper", "Whisper (local)", Arc::new(p));
                }
                Err(e) => tracing::warn!("failed to load whisper model: {e}"),
            }
        }
    }

    if let Ok(base_url) = std::env::var("VOXTRAL_BASE_URL") {
        let voxtral = Arc::new(simply_voice::VoxtralProvider::local(&base_url));
        voice = voice
            .register_stt("voxtral", "Voxtral (local)", voxtral.clone())
            .register_tts("voxtral", "Voxtral (local)", voxtral);
        tracing::info!(base_url = %base_url, "voxtral local STT + TTS registered");
    } else if let Some(key) = settings.get_api_key("mistral")
        .or_else(|| std::env::var("MISTRAL_API_KEY").ok()) {
        let voxtral = Arc::new(simply_voice::VoxtralProvider::new(key));
        voice = voice
            .register_stt("voxtral", "Voxtral (Mistral)", voxtral.clone())
            .register_tts("voxtral", "Voxtral (Mistral)", voxtral);
        tracing::info!("voxtral STT + TTS registered (Mistral API)");
    }

    if let Some(key) = settings.get_api_key("elevenlabs")
        .or_else(|| std::env::var("ELEVENLABS_API_KEY").ok()) {
        let elevenlabs = Arc::new(simply_voice::ElevenLabsProvider::new(key));
        voice = voice
            .register_stt("elevenlabs", "ElevenLabs", elevenlabs.clone())
            .register_tts("elevenlabs", "ElevenLabs", elevenlabs);
        tracing::info!("elevenlabs STT + TTS registered");
    }

    if let Some(key) = settings.get_api_key("google")
        .or_else(|| std::env::var("GOOGLE_API_KEY").ok()) {
        let gemini = Arc::new(simply_voice::GeminiRealtimeProvider::new(key));
        voice = voice.register_realtime("gemini", "Gemini Realtime", gemini);
        tracing::info!("gemini realtime registered");
    }

    voice
}

/// Create an embedding provider from config.
pub fn create_embedding_provider(
    config: &config::EmbeddingConfig,
    settings: &config::Settings,
) -> anyhow::Result<Arc<dyn llm::EmbeddingProvider>> {
    match config.provider.as_str() {
        "local" => Ok(Arc::new(llm::providers::LocalEmbeddingProvider::new(&config.model)?)),
        "ollama" => {
            let provider = match std::env::var("OLLAMA_BASE_URL") {
                Ok(base_url) => llm::providers::OllamaProvider::new(&base_url),
                Err(_) => llm::providers::OllamaProvider::default(),
            };
            Ok(Arc::new(provider.create_embedding_provider(&config.model)))
        }
        "mistral" => {
            let api_key = settings.get_api_key("mistral")
                .or_else(|| std::env::var("MISTRAL_API_KEY").ok())
                .ok_or_else(|| anyhow::anyhow!("mistral API key not configured"))?;
            Ok(Arc::new(llm::providers::MistralProvider::default(&api_key).create_embedding_provider(&config.model)))
        }
        "gemini" => {
            let api_key = settings.get_api_key("google")
                .or_else(|| std::env::var("GOOGLE_API_KEY").ok())
                .or_else(|| settings.get_api_key("gemini"))
                .or_else(|| std::env::var("GEMINI_API_KEY").ok())
                .ok_or_else(|| anyhow::anyhow!("google/gemini API key not configured"))?;
            Ok(Arc::new(llm::providers::GeminiProvider::default(&api_key).create_embedding_provider(&config.model)))
        }
        "claude" | "voyage" => {
            let api_key = settings.get_api_key("voyage")
                .or_else(|| std::env::var("VOYAGE_API_KEY").ok())
                .ok_or_else(|| anyhow::anyhow!("voyage API key not configured"))?;
            Ok(Arc::new(llm::providers::VoyageEmbeddingProvider::new(&api_key, config.model.clone())))
        }
        other => anyhow::bail!("unknown embedding provider: {other}"),
    }
}
