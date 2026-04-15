//! Extracted service implementations for individual API traits.
//!
//! Each service owns only the state it needs and implements its API trait directly.
//! `EmbeddedDaemon` holds these services and delegates to them for `DaemonApi` compat.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};

use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::traits::{StorageTypes, Stores};
use simply_core::storage::DocumentResolver;

use crate::api::*;

// ---------------------------------------------------------------------------
// ModelService
// ---------------------------------------------------------------------------

pub struct ModelService {
    default_model_id: Mutex<String>,
    cached_models: Mutex<Option<Vec<llm::ModelInfo>>>,
}

impl ModelService {
    pub fn new(default_model_id: String) -> Self {
        Self {
            default_model_id: Mutex::new(default_model_id),
            cached_models: Mutex::new(None),
        }
    }

    pub async fn default_model(&self) -> String {
        self.default_model_id.lock().await.clone()
    }

    async fn fetch_all_models(&self) -> Vec<llm::ModelInfo> {
        let mut all = Vec::new();
        for (provider, result) in llm::list_all_models().await {
            match result {
                Ok(models) => all.extend(models),
                Err(e) => tracing::warn!(provider, error = %e, "failed to fetch models"),
            }
        }
        *self.cached_models.lock().await = Some(all.clone());
        all
    }
}

#[async_trait]
impl ModelApi for ModelService {
    async fn list_models(&self) -> anyhow::Result<Vec<llm::ModelInfo>> {
        if let Some(cached) = self.cached_models.lock().await.clone() {
            return Ok(cached);
        }
        Ok(self.fetch_all_models().await)
    }

    async fn list_providers(&self) -> Vec<llm::ProviderInfo> {
        llm::list_providers()
    }

    async fn default_model_id(&self) -> String {
        self.default_model_id.lock().await.clone()
    }

    async fn set_default_model(&self, model_id: &str) -> anyhow::Result<()> {
        let _ = llm::create_model(model_id)?;
        *self.default_model_id.lock().await = model_id.to_string();
        // Invalidate cache when model changes (provider config may have changed)
        *self.cached_models.lock().await = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AssetService
// ---------------------------------------------------------------------------

pub struct AssetService<S: StorageTypes> {
    coordinator: Arc<StorageCoordinator<S>>,
    stores: Arc<dyn Stores<S>>,
}

impl<S: StorageTypes> AssetService<S> {
    pub fn new(coordinator: Arc<StorageCoordinator<S>>, stores: Arc<dyn Stores<S>>) -> Self {
        Self { coordinator, stores }
    }
}

#[async_trait]
impl<S: StorageTypes> AssetApi for AssetService<S>
where
    S::Document: DocumentResolver,
{
    async fn store_asset(&self, upload: simply_rpc::BinaryUpload) -> anyhow::Result<AssetInfo> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use simply_core::storage::traits::AssetStore;
        let b64 = STANDARD.encode(&upload.data);
        let id = self.coordinator.store_asset(&b64, &upload.mime_type).await?;
        let stored = self.stores.asset().get(&id).await?
            .ok_or_else(|| anyhow::anyhow!("asset not found after store"))?;
        Ok(AssetInfo {
            id,
            blob_hash: stored.blob_hash.clone(),
            mime_type: stored.mime_type.clone(),
            size_bytes: stored.size_bytes,
        })
    }

    async fn list_assets(&self) -> anyhow::Result<Vec<AssetId>> {
        use simply_core::storage::traits::AssetStore;
        self.stores.asset().list().await
    }

    async fn get_asset_info(&self, id: &AssetId) -> anyhow::Result<AssetInfo> {
        use simply_core::storage::traits::AssetStore;
        let stored = self.stores.asset().get(id).await?
            .ok_or_else(|| anyhow::anyhow!("asset not found: {id}"))?;
        Ok(AssetInfo {
            id: id.clone(),
            blob_hash: stored.blob_hash.clone(),
            mime_type: stored.mime_type.clone(),
            size_bytes: stored.size_bytes,
        })
    }

    async fn get_asset(&self, id: &AssetId) -> anyhow::Result<simply_rpc::BinaryResponse> {
        use simply_core::storage::traits::AssetStore;
        let stored = self.stores.asset().get(id).await?
            .ok_or_else(|| anyhow::anyhow!("asset not found: {id}"))?;
        let data = self.coordinator.get_blob(&stored.blob_hash).await?;
        Ok(simply_rpc::BinaryResponse { data, mime_type: stored.mime_type.clone() })
    }

    async fn get_blob(&self, hash: &simply_core::storage::types::BlobHash) -> anyhow::Result<simply_rpc::BinaryResponse> {
        use simply_core::storage::traits::AssetStore;
        let data = self.coordinator.get_blob(hash).await?;
        let mime_type = self.stores.asset()
            .get_by_blob_hash(hash).await?
            .map(|a| a.mime_type.clone())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        Ok(simply_rpc::BinaryResponse { data, mime_type })
    }
}

// ---------------------------------------------------------------------------
// DocumentService
// ---------------------------------------------------------------------------

pub struct DocumentService<S: StorageTypes> {
    stores: Arc<dyn Stores<S>>,
    user_id: simply_core::storage::ids::UserId,
}

impl<S: StorageTypes> DocumentService<S> {
    pub fn new(stores: Arc<dyn Stores<S>>, user_id: simply_core::storage::ids::UserId) -> Self {
        Self { stores, user_id }
    }

    /// Verify the current user can access a tab's parent document.
    async fn verify_tab_access(&self, tab_id: &simply_core::storage::ids::TabId, require_owner: bool) -> anyhow::Result<()> {
        use simply_core::storage::traits::DocumentStore;
        let tab = self.stores.document().get_document_tab(tab_id).await?
            .ok_or_else(|| anyhow::anyhow!("tab not found: {tab_id}"))?;
        self.verify_document_access(&tab.document_id, require_owner).await
    }

    /// Verify the current user owns this document or it's public.
    /// `require_owner` = true rejects public docs too (for mutations).
    async fn verify_document_access(&self, document_id: &simply_core::storage::ids::DocumentId, require_owner: bool) -> anyhow::Result<()> {
        use simply_core::storage::traits::DocumentStore;
        let doc = self.stores.document().get_document(document_id).await?
            .ok_or_else(|| anyhow::anyhow!("document not found: {document_id}"))?;
        if doc.user_id == self.user_id {
            return Ok(());
        }
        if !require_owner && doc.is_public {
            return Ok(());
        }
        anyhow::bail!("access denied: document belongs to another user");
    }
}

#[async_trait]
impl<S: StorageTypes> DocumentApi for DocumentService<S> {
    async fn list_documents(&self) -> anyhow::Result<Vec<DocumentInfo>> {
        use simply_core::storage::traits::DocumentStore;
        let docs = self.stores.document().list_documents(&self.user_id).await?;
        let mut result = Vec::new();
        for doc in docs {
            let tabs = self.stores.document().list_document_tabs(&doc.id).await?;
            result.push(DocumentInfo {
                id: doc.id.to_string(),
                title: doc.title.clone(),
                document_type: doc.document_type.clone(),
                source: format!("{:?}", doc.source),
                source_id: doc.source_id.clone(),
                tab_count: tabs.len(),
                created_at: doc.content.created_at,
                updated_at: doc.content.updated_at,
            });
        }
        Ok(result)
    }

    async fn search_documents(&self, query: &str) -> anyhow::Result<Vec<DocumentInfo>> {
        use simply_core::storage::traits::DocumentStore;
        let docs = self.stores.document().search_documents(&self.user_id, query, 50).await?;
        let mut result = Vec::new();
        for doc in docs {
            let tabs = self.stores.document().list_document_tabs(&doc.id).await?;
            result.push(DocumentInfo {
                id: doc.id.to_string(),
                title: doc.title.clone(),
                document_type: doc.document_type.clone(),
                source: format!("{:?}", doc.source),
                source_id: doc.source_id.clone(),
                tab_count: tabs.len(),
                created_at: doc.content.created_at,
                updated_at: doc.content.updated_at,
            });
        }
        Ok(result)
    }

    async fn get_document(&self, document_id: &str) -> anyhow::Result<DocumentDetail> {
        use simply_core::storage::ids::DocumentId;
        use simply_core::storage::traits::DocumentStore;

        let id = DocumentId::from_string(document_id);
        self.verify_document_access(&id, false).await?;
        let doc = self.stores.document().get_document(&id).await?
            .ok_or_else(|| anyhow::anyhow!("document not found: {document_id}"))?;

        let tabs = self.stores.document().list_document_tabs(&id).await?;
        let tab_infos: Vec<TabInfo> = tabs.iter().map(|t| TabInfo {
            id: t.id.to_string(),
            title: t.title.clone(),
            icon: t.icon.clone(),
            parent_tab_id: t.parent_tab_id.as_ref().map(|id| id.to_string()),
            tab_index: t.tab_index,
            content_markdown: t.content_markdown.clone(),
            created_at: t.content.created_at,
            updated_at: t.content.updated_at,
        }).collect();

        Ok(DocumentDetail {
            id: doc.id.to_string(),
            title: doc.title.clone(),
            document_type: doc.document_type.clone(),
            source: format!("{:?}", doc.source),
            source_id: doc.source_id.clone(),
            tabs: tab_infos,
            created_at: doc.content.created_at,
            updated_at: doc.content.updated_at,
        })
    }

    async fn create_document(&self, request: CreateDocumentRequest) -> anyhow::Result<DocumentInfo> {
        use simply_core::storage::traits::DocumentStore;
        use simply_core::storage::types::DocumentSource;

        let doc_id = self.stores.document().create_document(
            &self.user_id,
            &request.title,
            request.document_type.as_deref().unwrap_or(simply_core::storage::types::DocumentType::DOCUMENT),
            DocumentSource::UserCreated,
            None,
        ).await?;

        // Create initial tab if content provided
        if let Some(ref content) = request.content {
            self.stores.document().create_document_tab(
                &doc_id,
                None, // no parent
                0,    // first tab
                &request.title,
                None, // no icon
                Some(content),
                &[],  // no assets
                None, // no source tab
            ).await?;
        }

        let doc = self.stores.document().get_document(&doc_id).await?
            .ok_or_else(|| anyhow::anyhow!("document not found after create"))?;
        let tabs = self.stores.document().list_document_tabs(&doc_id).await?;

        Ok(DocumentInfo {
            id: doc.id.to_string(),
            title: doc.title.clone(),
            source: format!("{:?}", doc.source),
            source_id: doc.source_id.clone(),
            tab_count: tabs.len(),
            created_at: doc.content.created_at,
            updated_at: doc.content.updated_at,
        })
    }

    async fn rename_document(&self, document_id: &str, title: &str) -> anyhow::Result<()> {
        use simply_core::storage::ids::DocumentId;
        use simply_core::storage::traits::DocumentStore;
        let id = DocumentId::from_string(document_id);
        self.verify_document_access(&id, true).await?;
        self.stores.document().update_document_title(&id, title).await
    }

    async fn delete_document(&self, document_id: &str) -> anyhow::Result<()> {
        use simply_core::storage::ids::DocumentId;
        use simply_core::storage::traits::DocumentStore;
        let id = DocumentId::from_string(document_id);
        self.verify_document_access(&id, true).await?;
        self.stores.document().delete_document(&id).await?;
        Ok(())
    }

    async fn create_tab(&self, document_id: &str, request: CreateTabRequest) -> anyhow::Result<TabInfo> {
        use simply_core::storage::ids::{DocumentId, TabId};
        use simply_core::storage::traits::DocumentStore;

        let doc_id = DocumentId::from_string(document_id);
        self.verify_document_access(&doc_id, true).await?;
        let parent = request.parent_tab_id.as_deref().map(TabId::from_string);

        let tab_id = self.stores.document().create_document_tab(
            &doc_id,
            parent.as_ref(),
            request.tab_index.unwrap_or(0),
            &request.title,
            None,
            request.content.as_deref(),
            &[],
            None,
        ).await?;

        let tab = self.stores.document().get_document_tab(&tab_id).await?
            .ok_or_else(|| anyhow::anyhow!("tab not found after create"))?;

        Ok(TabInfo {
            id: tab.id.to_string(),
            title: tab.title.clone(),
            icon: tab.icon.clone(),
            parent_tab_id: tab.parent_tab_id.as_ref().map(|id| id.to_string()),
            tab_index: tab.tab_index,
            content_markdown: tab.content_markdown.clone(),
            created_at: tab.content.created_at,
            updated_at: tab.content.updated_at,
        })
    }

    async fn get_tab(&self, tab_id: &str) -> anyhow::Result<TabInfo> {
        use simply_core::storage::ids::TabId;
        use simply_core::storage::traits::DocumentStore;

        let tid = TabId::from_string(tab_id);
        self.verify_tab_access(&tid, false).await?;
        let tab = self.stores.document().get_document_tab(&tid).await?
            .ok_or_else(|| anyhow::anyhow!("tab not found: {tab_id}"))?;

        Ok(TabInfo {
            id: tab.id.to_string(),
            title: tab.title.clone(),
            icon: tab.icon.clone(),
            parent_tab_id: tab.parent_tab_id.as_ref().map(|id| id.to_string()),
            tab_index: tab.tab_index,
            content_markdown: tab.content_markdown.clone(),
            created_at: tab.content.created_at,
            updated_at: tab.content.updated_at,
        })
    }

    async fn update_tab(&self, tab_id: &str, request: UpdateTabRequest) -> anyhow::Result<()> {
        use simply_core::storage::ids::TabId;
        use simply_core::storage::traits::DocumentStore;
        let tid = TabId::from_string(tab_id);
        self.verify_tab_access(&tid, true).await?;
        self.stores.document().update_document_tab_content(&tid, &request.content, &[]).await
    }

    async fn delete_tab(&self, tab_id: &str) -> anyhow::Result<()> {
        use simply_core::storage::ids::TabId;
        use simply_core::storage::traits::DocumentStore;
        let tid = TabId::from_string(tab_id);
        self.verify_tab_access(&tid, true).await?;
        self.stores.document().delete_document_tab(&tid).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// VoiceService
// ---------------------------------------------------------------------------

use std::collections::HashMap;

struct RegisteredProvider {
    info: VoiceProviderInfo,
    stt: Option<Arc<dyn simply_voice::SttProvider>>,
    tts: Option<Arc<dyn simply_voice::TtsProvider>>,
    realtime: Option<Arc<dyn simply_voice::RealtimeProvider>>,
}

pub struct VoiceService {
    providers: HashMap<String, RegisteredProvider>,
}

impl VoiceService {
    pub fn new() -> Self {
        Self { providers: HashMap::new() }
    }

    pub fn register_stt(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        provider: Arc<dyn simply_voice::SttProvider>,
    ) -> Self {
        let id = id.into();
        let entry = self.providers.entry(id.clone()).or_insert_with(|| RegisteredProvider {
            info: VoiceProviderInfo { id, name: name.into(), capabilities: Vec::new() },
            stt: None, tts: None, realtime: None,
        });
        entry.stt = Some(provider);
        if !entry.info.capabilities.contains(&"stt".to_string()) {
            entry.info.capabilities.push("stt".to_string());
        }
        self
    }

    pub fn register_tts(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        provider: Arc<dyn simply_voice::TtsProvider>,
    ) -> Self {
        let id = id.into();
        let entry = self.providers.entry(id.clone()).or_insert_with(|| RegisteredProvider {
            info: VoiceProviderInfo { id, name: name.into(), capabilities: Vec::new() },
            stt: None, tts: None, realtime: None,
        });
        entry.tts = Some(provider);
        if !entry.info.capabilities.contains(&"tts".to_string()) {
            entry.info.capabilities.push("tts".to_string());
        }
        self
    }

    pub fn register_realtime(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        provider: Arc<dyn simply_voice::RealtimeProvider>,
    ) -> Self {
        let id = id.into();
        let entry = self.providers.entry(id.clone()).or_insert_with(|| RegisteredProvider {
            info: VoiceProviderInfo { id, name: name.into(), capabilities: Vec::new() },
            stt: None, tts: None, realtime: None,
        });
        entry.realtime = Some(provider);
        if !entry.info.capabilities.contains(&"realtime".to_string()) {
            entry.info.capabilities.push("realtime".to_string());
        }
        self
    }
}

/// Spawn the STT pipeline: VoiceInput → VAD → SttProvider → VoiceEvents.
fn spawn_stt_pipeline(
    stt: Arc<dyn simply_voice::SttProvider>,
    mut input_rx: mpsc::Receiver<simply_voice::VoiceInput>,
    event_tx: mpsc::Sender<simply_voice::VoiceEvent>,
) {
    tokio::spawn(async move {
        use simply_voice::{VadEvent, VoiceActivityDetector, VoiceEvent, VoiceInput, AudioChunk};

        let mut vad = VoiceActivityDetector::new();

        while let Some(input) = input_rx.recv().await {
            let chunk = match input {
                VoiceInput::Audio(c) => c,
            };

            let samples: Vec<i16> = chunk.data.chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();

            if let Some(vad_event) = vad.process(&samples) {
                match vad_event {
                    VadEvent::SpeechStart => {
                        let _ = event_tx.send(VoiceEvent::Listening).await;
                    }
                    VadEvent::SpeechChunk(_) => {}
                    VadEvent::SpeechEnd(audio_samples) => {
                        tracing::info!(samples = audio_samples.len(), "speech ended, transcribing");
                        let _ = event_tx.send(VoiceEvent::Transcribing).await;

                        let bytes: Vec<u8> = audio_samples.iter()
                            .flat_map(|s| s.to_le_bytes())
                            .collect();
                        let audio = AudioChunk::new(bytes);

                        match stt.transcribe(audio).await {
                            Ok(t) if !t.text.trim().is_empty() => {
                                tracing::info!(text = %t.text, "STT transcription");
                                let _ = event_tx.send(VoiceEvent::UserTranscript(t.text)).await;
                            }
                            Ok(_) => {
                                tracing::debug!("STT: empty transcription");
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "STT transcription failed");
                                let _ = event_tx.send(VoiceEvent::Error(format!("STT failed: {e}"))).await;
                            }
                        }
                    }
                }
            }
        }
    });
}

/// Spawn the realtime pipeline: VoiceInput → RealtimeProvider → VoiceEvents.
fn spawn_realtime_pipeline(
    realtime: Arc<dyn simply_voice::RealtimeProvider>,
    mut input_rx: mpsc::Receiver<simply_voice::VoiceInput>,
    event_tx: mpsc::Sender<simply_voice::VoiceEvent>,
) {
    tokio::spawn(async move {
        use simply_voice::{RealtimeConfig, RealtimeEvent, RealtimeInput, VoiceEvent, VoiceInput};

        let config = RealtimeConfig::default();
        let (rt_tx, mut rt_rx) = match realtime.connect(config).await {
            Ok(pair) => pair,
            Err(e) => {
                let _ = event_tx.send(VoiceEvent::Error(format!("Realtime connect failed: {e}"))).await;
                return;
            }
        };

        // Forward input to realtime provider
        let rt_tx_clone = rt_tx.clone();
        tokio::spawn(async move {
            while let Some(input) = input_rx.recv().await {
                let rt_input = match input {
                    VoiceInput::Audio(chunk) => RealtimeInput::Audio(chunk),
                };
                if rt_tx_clone.send(rt_input).await.is_err() {
                    break;
                }
            }
        });

        // Forward realtime events to voice events
        while let Some(event) = rt_rx.recv().await {
            let voice_event = match event {
                RealtimeEvent::Audio(chunk) => VoiceEvent::Audio(chunk),
                RealtimeEvent::ModelTranscript(text) => VoiceEvent::ModelTranscript(text),
                RealtimeEvent::UserTranscript(text) => VoiceEvent::UserTranscript(text),
                RealtimeEvent::TurnEnd => VoiceEvent::TurnEnd,
            };
            if event_tx.send(voice_event).await.is_err() {
                break;
            }
        }
    });
}

#[async_trait]
impl VoiceApi for VoiceService {
    async fn list_voice_providers(&self) -> anyhow::Result<Vec<VoiceProviderInfo>> {
        Ok(self.providers.values().map(|p| p.info.clone()).collect())
    }

    async fn voice_connect(&self, provider_id: &str) -> anyhow::Result<simply_rpc::StreamHandle<simply_voice::VoiceInput, simply_voice::VoiceEvent>> {
        tracing::info!(provider_id, "voice_connect");
        let provider = self.providers.get(provider_id)
            .ok_or_else(|| anyhow::anyhow!("unknown voice provider: {provider_id}"))?;

        let (input_tx, input_rx) = mpsc::channel::<simply_voice::VoiceInput>(64);
        let (event_tx, event_rx) = mpsc::channel::<simply_voice::VoiceEvent>(64);

        // Prefer realtime if available, fall back to STT pipeline
        if let Some(ref realtime) = provider.realtime {
            spawn_realtime_pipeline(Arc::clone(realtime), input_rx, event_tx);
        } else if let Some(ref stt) = provider.stt {
            spawn_stt_pipeline(Arc::clone(stt), input_rx, event_tx);
        } else {
            anyhow::bail!("provider '{provider_id}' has no STT or realtime capability");
        }

        Ok(simply_rpc::StreamHandle::new(input_tx, event_rx))
    }

    async fn synthesize(&self, text: &str, provider_id: &str, voice: &str) -> anyhow::Result<simply_voice::Audio> {
        tracing::info!(provider_id, voice, text_len = text.len(), "synthesize called");
        let provider = self.providers.get(provider_id)
            .ok_or_else(|| anyhow::anyhow!("unknown voice provider: {provider_id}"))?;
        let tts = provider.tts.as_ref()
            .ok_or_else(|| anyhow::anyhow!("provider '{provider_id}' has no TTS capability"))?;
        let result = tts.synthesize(text, voice).await;
        tracing::info!(ok = result.is_ok(), "synthesize done");
        result
    }

    async fn list_voices(&self, provider_id: &str) -> anyhow::Result<Vec<simply_voice::Voice>> {
        let provider = self.providers.get(provider_id)
            .ok_or_else(|| anyhow::anyhow!("unknown voice provider: {provider_id}"))?;
        let tts = provider.tts.as_ref()
            .ok_or_else(|| anyhow::anyhow!("provider '{provider_id}' has no TTS capability"))?;
        tts.voices().await
    }

    async fn voice_disconnect(&self, _session_id: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// CoreService
// ---------------------------------------------------------------------------

pub struct CoreService {
    kill_tx: Option<tokio::sync::mpsc::Sender<()>>,
}

impl CoreService {
    pub fn new(kill_tx: tokio::sync::mpsc::Sender<()>) -> Self {
        Self { kill_tx: Some(kill_tx) }
    }

    pub fn embedded() -> Self {
        Self { kill_tx: None }
    }
}

#[async_trait]
impl CoreApi for CoreService {
    async fn health(&self) -> anyhow::Result<DaemonHealth> {
        Ok(DaemonHealth { status: "ok".to_string() })
    }

    async fn kill(&self) -> anyhow::Result<()> {
        if let Some(tx) = &self.kill_tx {
            let _ = tx.send(()).await;
        }
        Ok(())
    }

    async fn version(&self) -> anyhow::Result<String> {
        Ok(env!("CARGO_PKG_VERSION").to_string())
    }
}
