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

use simply_rpc::RequestContext;

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
    async fn store_asset(&self, _ctx: &RequestContext, upload: simply_rpc::BinaryUpload) -> anyhow::Result<AssetInfo> {
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

    async fn list_assets(&self, _ctx: &RequestContext) -> anyhow::Result<Vec<AssetId>> {
        use simply_core::storage::traits::AssetStore;
        self.stores.asset().list().await
    }

    async fn get_asset_info(&self, _ctx: &RequestContext, id: &AssetId) -> anyhow::Result<AssetInfo> {
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

    async fn get_asset(&self, _ctx: &RequestContext, id: &AssetId) -> anyhow::Result<simply_rpc::BinaryResponse> {
        use simply_core::storage::traits::AssetStore;
        let stored = self.stores.asset().get(id).await?
            .ok_or_else(|| anyhow::anyhow!("asset not found: {id}"))?;
        let data = self.coordinator.get_blob(&stored.blob_hash).await?;
        Ok(simply_rpc::BinaryResponse { data, mime_type: stored.mime_type.clone() })
    }

    async fn get_blob(&self, _ctx: &RequestContext, hash: &simply_core::storage::types::BlobHash) -> anyhow::Result<simply_rpc::BinaryResponse> {
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
    embedding_queue: Option<Arc<dyn crate::embedding_queue::EmbeddingQueue>>,
    vector_store: Option<Arc<dyn simply_core::embedding::VectorStore>>,
}

impl<S: StorageTypes> DocumentService<S> {
    pub fn new(stores: Arc<dyn Stores<S>>) -> Self {
        Self { stores, embedding_queue: None, vector_store: None }
    }

    /// Attach an embedding queue and vector store for automatic indexing.
    pub fn with_embedding(
        mut self,
        queue: Arc<dyn crate::embedding_queue::EmbeddingQueue>,
        vector_store: Arc<dyn simply_core::embedding::VectorStore>,
    ) -> Self {
        self.embedding_queue = Some(queue);
        self.vector_store = Some(vector_store);
        self
    }

    /// Extract user_id from RequestContext, failing if not authenticated.
    fn require_user(ctx: &RequestContext) -> anyhow::Result<simply_core::storage::ids::UserId> {
        ctx.scope.user_id.as_ref()
            .map(|id| simply_core::storage::ids::UserId::from_string(id))
            .ok_or_else(|| anyhow::anyhow!("authentication required"))
    }

    /// Enqueue a tab for embedding if a queue is configured.
    async fn enqueue_embedding(&self, tab_id: &simply_core::storage::ids::TabId, document_id: &simply_core::storage::ids::DocumentId, document_type: &str, user_id: &simply_core::storage::ids::UserId, text: &str) {
        if let Some(ref queue) = self.embedding_queue {
            if !text.is_empty() {
                queue.enqueue(crate::embedding_queue::EmbedJob {
                    tab_id: tab_id.clone(),
                    document_id: document_id.clone(),
                    document_type: document_type.to_string(),
                    user_id: user_id.clone(),
                    text: text.to_string(),
                }).await;
            }
        }
    }

    /// Verify the user can access a tab's parent document.
    async fn verify_tab_access(&self, user_id: &simply_core::storage::ids::UserId, tab_id: &simply_core::storage::ids::TabId, require_owner: bool) -> anyhow::Result<()> {
        use simply_core::storage::traits::DocumentStore;
        let tab = self.stores.document().get_document_tab(tab_id).await?
            .ok_or_else(|| anyhow::anyhow!("tab not found: {tab_id}"))?;
        self.verify_document_access(user_id, &tab.document_id, require_owner).await
    }

    /// Verify the user owns this document or it's public.
    async fn verify_document_access(&self, user_id: &simply_core::storage::ids::UserId, document_id: &simply_core::storage::ids::DocumentId, require_owner: bool) -> anyhow::Result<()> {
        use simply_core::storage::traits::DocumentStore;
        let doc = self.stores.document().get_document(document_id).await?
            .ok_or_else(|| anyhow::anyhow!("document not found: {document_id}"))?;
        if doc.user_id == *user_id {
            return Ok(());
        }
        if !require_owner && doc.is_public {
            return Ok(());
        }
        anyhow::bail!("access denied: document belongs to another user");
    }

    /// Delete a document, its tabs, and all associated embeddings.
    /// Does not check ownership — caller must verify access first.
    async fn delete_document_with_embeddings(&self, id: &simply_core::storage::ids::DocumentId) -> anyhow::Result<()> {
        use simply_core::storage::traits::DocumentStore;

        // Delete embeddings first (tabs are about to disappear)
        if let Some(ref vs) = self.vector_store {
            vs.delete_by_document(id).await?;
        }
        // Delete tabs explicitly (don't rely on CASCADE)
        let tabs = self.stores.document().list_document_tabs(id).await?;
        for tab in &tabs {
            self.stores.document().delete_document_tab(&tab.id).await?;
        }
        self.stores.document().delete_document(id).await?;
        Ok(())
    }

    async fn docs_to_infos(&self, docs: Vec<simply_core::storage::traits::StoredDocument>) -> anyhow::Result<Vec<DocumentInfo>> {
        use simply_core::storage::traits::DocumentStore;
        let mut result = Vec::new();
        for doc in docs {
            let tabs = self.stores.document().list_document_tabs(&doc.id).await?;
            result.push(DocumentInfo {
                id: doc.id.to_string(),
                user_id: doc.user_id.to_string(),
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
}

#[async_trait]
impl<S: StorageTypes> DocumentApi for DocumentService<S> {
    async fn list_documents(&self, ctx: &RequestContext) -> anyhow::Result<Vec<DocumentInfo>> {
        use simply_core::storage::traits::DocumentStore;
        let user_id = Self::require_user(ctx)?;
        let docs = self.stores.document().list_documents(&user_id).await?;
        self.docs_to_infos(docs).await
    }

    async fn list_all_documents(&self) -> anyhow::Result<Vec<DocumentInfo>> {
        use simply_core::storage::traits::DocumentStore;
        let docs = self.stores.document().list_all_documents().await?;
        self.docs_to_infos(docs).await
    }

    async fn search_documents(&self, ctx: &RequestContext, query: &str) -> anyhow::Result<Vec<DocumentInfo>> {
        use simply_core::storage::traits::DocumentStore;
        let user_id = Self::require_user(ctx)?;
        let docs = self.stores.document().search_documents(&user_id, query, 50).await?;
        self.docs_to_infos(docs).await
    }

    async fn get_document(&self, ctx: &RequestContext, document_id: &str) -> anyhow::Result<DocumentDetail> {
        use simply_core::storage::ids::DocumentId;
        use simply_core::storage::traits::DocumentStore;

        let user_id = Self::require_user(ctx)?;
        let id = DocumentId::from_string(document_id);
        self.verify_document_access(&user_id, &id, false).await?;
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

    async fn create_document(&self, ctx: &RequestContext, request: CreateDocumentRequest) -> anyhow::Result<DocumentInfo> {
        use simply_core::storage::traits::DocumentStore;
        use simply_core::storage::types::DocumentSource;

        let user_id = Self::require_user(ctx)?;
        let source = if request.source_id.is_some() { DocumentSource::GoogleDrive } else { DocumentSource::UserCreated };

        // If source_id is set, delete any existing document with the same source
        // (re-import replaces the old version, including its embeddings)
        if let Some(ref sid) = request.source_id {
            if let Some(existing) = self.stores.document().get_document_by_source(&user_id, source.clone(), sid).await? {
                self.delete_document_with_embeddings(&existing.id).await?;
            }
        }

        let doc_id = self.stores.document().create_document(
            &user_id,
            &request.title,
            request.document_type.as_deref().unwrap_or(simply_core::storage::types::DocumentType::DOCUMENT),
            source,
            request.source_id.as_deref(),
        ).await?;

        // Create initial tab if content provided
        if let Some(ref content) = request.content {
            let tab_id = self.stores.document().create_document_tab(
                &doc_id,
                None, // no parent
                0,    // first tab
                &request.title,
                None, // no icon
                Some(content),
                &[],  // no assets
                None, // no source tab
            ).await?;

            let doc_type = request.document_type.as_deref().unwrap_or(simply_core::storage::types::DocumentType::DOCUMENT);
            self.enqueue_embedding(&tab_id, &doc_id, doc_type, &user_id, content).await;
        }

        let doc = self.stores.document().get_document(&doc_id).await?
            .ok_or_else(|| anyhow::anyhow!("document not found after create"))?;

        self.docs_to_infos(vec![doc]).await.map(|mut v| v.remove(0))
    }

    async fn rename_document(&self, ctx: &RequestContext, document_id: &str, title: &str) -> anyhow::Result<()> {
        use simply_core::storage::ids::DocumentId;
        use simply_core::storage::traits::DocumentStore;
        let user_id = Self::require_user(ctx)?;
        let id = DocumentId::from_string(document_id);
        self.verify_document_access(&user_id, &id, true).await?;
        self.stores.document().update_document_title(&id, title).await
    }

    async fn delete_document(&self, ctx: &RequestContext, document_id: &str) -> anyhow::Result<()> {
        use simply_core::storage::ids::DocumentId;
        let user_id = Self::require_user(ctx)?;
        let id = DocumentId::from_string(document_id);
        self.verify_document_access(&user_id, &id, true).await?;
        self.delete_document_with_embeddings(&id).await
    }

    async fn create_tab(&self, ctx: &RequestContext, document_id: &str, request: CreateTabRequest) -> anyhow::Result<TabInfo> {
        use simply_core::storage::ids::{DocumentId, TabId};
        use simply_core::storage::traits::DocumentStore;

        let user_id = Self::require_user(ctx)?;
        let doc_id = DocumentId::from_string(document_id);
        self.verify_document_access(&user_id, &doc_id, true).await?;
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

        // Enqueue for embedding
        if let Some(ref content) = request.content {
            let doc = self.stores.document().get_document(&doc_id).await?;
            let doc_type = doc.as_ref().map(|d| d.document_type.as_str()).unwrap_or("document");
            self.enqueue_embedding(&tab_id, &doc_id, doc_type, &user_id, content).await;
        }

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

    async fn get_tab(&self, ctx: &RequestContext, tab_id: &str) -> anyhow::Result<TabInfo> {
        use simply_core::storage::ids::TabId;
        use simply_core::storage::traits::DocumentStore;

        let user_id = Self::require_user(ctx)?;
        let tid = TabId::from_string(tab_id);
        self.verify_tab_access(&user_id, &tid, false).await?;
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

    async fn update_tab(&self, ctx: &RequestContext, tab_id: &str, request: UpdateTabRequest) -> anyhow::Result<()> {
        use simply_core::storage::ids::TabId;
        use simply_core::storage::traits::DocumentStore;
        let user_id = Self::require_user(ctx)?;
        let tid = TabId::from_string(tab_id);
        self.verify_tab_access(&user_id, &tid, true).await?;
        self.stores.document().update_document_tab_content(&tid, &request.content, &[]).await?;

        // Re-embed updated content
        let tab = self.stores.document().get_document_tab(&tid).await?;
        if let Some(tab) = tab {
            let doc = self.stores.document().get_document(&tab.document_id).await?;
            let doc_type = doc.as_ref().map(|d| d.document_type.as_str()).unwrap_or("document");
            self.enqueue_embedding(&tid, &tab.document_id, doc_type, &user_id, &request.content).await;
        }

        Ok(())
    }

    async fn delete_tab(&self, ctx: &RequestContext, tab_id: &str) -> anyhow::Result<()> {
        use simply_core::storage::ids::TabId;
        use simply_core::storage::traits::DocumentStore;
        let user_id = Self::require_user(ctx)?;
        let tid = TabId::from_string(tab_id);
        self.verify_tab_access(&user_id, &tid, true).await?;
        self.stores.document().delete_document_tab(&tid).await?;
        Ok(())
    }

    async fn flush_tab_embedding(&self, _ctx: &RequestContext, tab_id: &str) -> anyhow::Result<()> {
        if let Some(ref queue) = self.embedding_queue {
            queue.flush(tab_id).await;
        }
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

    async fn public_url(&self) -> anyhow::Result<String> {
        let settings = config::Settings::load();
        Ok(settings.public_url.unwrap_or_else(|| {
            let port = settings.daemon_port.unwrap_or(config::DEFAULT_DAEMON_PORT);
            format!("http://localhost:{port}")
        }))
    }
}

// ---------------------------------------------------------------------------
// UserService
// ---------------------------------------------------------------------------

pub struct UserService<S: StorageTypes> {
    stores: Arc<dyn Stores<S>>,
}

impl<S: StorageTypes> UserService<S> {
    pub fn new(stores: Arc<dyn Stores<S>>) -> Self {
        Self { stores }
    }
}

#[async_trait]
impl<S: StorageTypes> UserApi for UserService<S>
where
    S::User: simply_core::storage::traits::UserStore,
{
    async fn resolve_user(&self, _ctx: &RequestContext, external_id: String) -> anyhow::Result<Option<simply_rpc::Scope>> {
        use simply_core::storage::traits::UserStore;
        let user_store = self.stores.user();
        match user_store.resolve_external_user(&external_id).await? {
            Some(user_id) => Ok(Some(simply_rpc::Scope::user(user_id.as_str()))),
            None => Ok(None),
        }
    }

    async fn resolve_or_create_user(&self, _ctx: &RequestContext, external_id: String) -> anyhow::Result<simply_rpc::Scope> {
        use simply_core::storage::traits::UserStore;
        let user = self.stores.user().resolve_or_create_external_user(&external_id).await?;
        Ok(simply_rpc::Scope::user(user.id.as_str()))
    }
}

// ---------------------------------------------------------------------------
// SearchService
// ---------------------------------------------------------------------------

pub struct SearchService<S: StorageTypes> {
    embedding_provider: Arc<dyn llm::EmbeddingProvider>,
    vector_store: Arc<dyn simply_core::embedding::VectorStore>,
    embedding_queue: Arc<dyn crate::embedding_queue::EmbeddingQueue>,
    stores: Arc<dyn Stores<S>>,
}

impl<S: StorageTypes> SearchService<S> {
    pub fn new(
        embedding_provider: Arc<dyn llm::EmbeddingProvider>,
        vector_store: Arc<dyn simply_core::embedding::VectorStore>,
        embedding_queue: Arc<dyn crate::embedding_queue::EmbeddingQueue>,
        stores: Arc<dyn Stores<S>>,
    ) -> Self {
        Self { embedding_provider, vector_store, embedding_queue, stores }
    }
}

#[async_trait]
impl<S: StorageTypes> SearchApi for SearchService<S> {
    async fn search(&self, ctx: &RequestContext, request: SearchRequest) -> anyhow::Result<Vec<SearchHit>> {
        let top_k = request.top_k.unwrap_or(5);
        let user_id = ctx.scope.user_id.as_ref()
            .map(|id| simply_core::storage::ids::UserId::from_string(id));

        // Embed the query
        let embeddings = self.embedding_provider.embed(&[&request.query]).await?;
        let vector = embeddings.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("embedding provider returned no vectors"))?
            .vector;

        // Search
        let filter = simply_core::embedding::SearchFilter {
            document_type: request.document_type,
            user_id,
        };

        let results = self.vector_store.search(simply_core::embedding::SearchQuery {
            vector,
            top_k,
            filter: Some(filter),
        }).await?;

        // Enrich with document titles
        let mut hits = Vec::with_capacity(results.len());
        for result in results {
            use simply_core::storage::traits::DocumentStore;
            let title = self.stores.document()
                .get_document(&result.chunk.document_id).await?
                .map(|d| d.title.clone())
                .unwrap_or_else(|| "Untitled".to_string());

            hits.push(SearchHit {
                document_id: result.chunk.document_id.to_string(),
                document_title: title,
                document_type: result.chunk.document_type,
                tab_id: result.chunk.tab_id.to_string(),
                chunk_text: result.chunk.text,
                chunk_index: result.chunk.chunk_index,
                score: result.score,
            });
        }

        Ok(hits)
    }

    async fn reindex(&self, _ctx: &RequestContext) -> anyhow::Result<ReindexStatus> {
        use simply_core::storage::traits::DocumentStore;

        // Delete all existing vectors
        self.vector_store.delete_all().await?;

        // Scan all documents across all users and enqueue all tabs
        let docs = self.stores.document().list_all_documents().await?;
        let mut tabs_queued = 0;

        for doc in &docs {
            let tabs = self.stores.document().list_document_tabs(&doc.id).await?;
            for tab in tabs {
                if let Some(ref content) = tab.content_markdown {
                    if !content.is_empty() {
                        self.embedding_queue.enqueue(crate::embedding_queue::EmbedJob {
                            tab_id: tab.id.clone(),
                            document_id: doc.id.clone(),
                            document_type: doc.document_type.clone(),
                            user_id: doc.user_id.clone(),
                            text: content.clone(),
                        }).await;
                        tabs_queued += 1;
                    }
                }
            }
        }

        Ok(ReindexStatus {
            message: format!("Reindex started: {} tabs queued", tabs_queued),
            tabs_queued,
        })
    }

    async fn queue_status(&self, _ctx: &RequestContext) -> anyhow::Result<crate::embedding_queue::EmbeddingQueueStatus> {
        Ok(self.embedding_queue.status().await)
    }
}
