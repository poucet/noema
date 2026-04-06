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
// VoiceService
// ---------------------------------------------------------------------------

pub struct VoiceService {
    stt: Option<Arc<dyn simply_voice::SttProvider>>,
}

impl VoiceService {
    pub fn new(stt: Option<Arc<dyn simply_voice::SttProvider>>) -> Self {
        Self { stt }
    }
}

#[async_trait]
impl VoiceApi for VoiceService {
    async fn voice_connect(&self, _session_id: &SessionId) -> anyhow::Result<VoiceHandle> {
        let stt = self.stt.clone()
            .ok_or_else(|| anyhow::anyhow!("no STT provider available"))?;

        let (audio_tx, mut audio_rx) = mpsc::channel::<simply_voice::AudioChunk>(64);
        let (event_tx, event_rx) = mpsc::channel::<simply_voice::VoiceEvent>(64);

        tokio::spawn(async move {
            use simply_voice::{VadEvent, VoiceActivityDetector, VoiceEvent, AudioChunk};

            let mut vad = VoiceActivityDetector::new();

            while let Some(chunk) = audio_rx.recv().await {
                // Convert PCM16 LE bytes to i16 samples for VAD
                let samples: Vec<i16> = chunk.data.chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect();

                if let Some(vad_event) = vad.process(&samples) {
                    match vad_event {
                        VadEvent::SpeechStart => {
                            let _ = event_tx.send(VoiceEvent::Listening).await;
                        }
                        VadEvent::SpeechChunk(_) => {
                            // Intermediate — no action needed
                        }
                        VadEvent::SpeechEnd(audio_samples) => {
                            let _ = event_tx.send(VoiceEvent::Transcribing).await;

                            // Convert i16 samples back to PCM16 LE bytes
                            let bytes: Vec<u8> = audio_samples.iter()
                                .flat_map(|s| s.to_le_bytes())
                                .collect();
                            let audio = AudioChunk::new(bytes);

                            match stt.transcribe(audio).await {
                                Ok(t) if !t.text.trim().is_empty() => {
                                    let _ = event_tx.send(
                                        VoiceEvent::UserTranscript(t.text)
                                    ).await;
                                }
                                Ok(_) => {} // empty transcription
                                Err(e) => {
                                    let _ = event_tx.send(
                                        VoiceEvent::Error(format!("STT failed: {e}"))
                                    ).await;
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(VoiceHandle { audio_in: audio_tx, events: event_rx })
    }

    async fn voice_disconnect(&self, _session_id: &SessionId) -> anyhow::Result<()> {
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
