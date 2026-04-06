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
}

impl ModelService {
    pub fn new(default_model_id: String) -> Self {
        Self { default_model_id: Mutex::new(default_model_id) }
    }

    pub async fn default_model(&self) -> String {
        self.default_model_id.lock().await.clone()
    }
}

#[async_trait]
impl ModelApi for ModelService {
    async fn list_models(&self) -> anyhow::Result<Vec<llm::ModelInfo>> {
        let mut all = Vec::new();
        for (_, result) in llm::list_all_models().await {
            if let Ok(models) = result { all.extend(models); }
        }
        Ok(all)
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

    async fn get_asset(&self, id: &AssetId) -> anyhow::Result<AssetInfo> {
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

pub struct VoiceService;

#[async_trait]
impl VoiceApi for VoiceService {
    async fn voice_connect(&self, _session_id: &SessionId) -> anyhow::Result<VoiceHandle> {
        let (audio_tx, _) = mpsc::channel(32);
        let (_, voice_rx) = mpsc::channel(32);
        Ok(VoiceHandle { audio_in: audio_tx, events: voice_rx })
    }

    async fn voice_disconnect(&self, _session_id: &SessionId) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DaemonInfoService
// ---------------------------------------------------------------------------

pub struct DaemonInfoService {
    kill_tx: Option<tokio::sync::mpsc::Sender<()>>,
}

impl DaemonInfoService {
    pub fn new(kill_tx: tokio::sync::mpsc::Sender<()>) -> Self {
        Self { kill_tx: Some(kill_tx) }
    }

    pub fn embedded() -> Self {
        Self { kill_tx: None }
    }
}

#[async_trait]
impl DaemonInfoApi for DaemonInfoService {
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
