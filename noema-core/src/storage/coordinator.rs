//! Storage coordinator - orchestrates blob, asset, and session storage
//!
//! The coordinator provides a unified interface for storing messages with assets.
//! It automatically extracts inline binary content (images, audio), stores them
//! in blob storage, records metadata in asset storage, and converts the content
//! to asset references before persisting to the session store.

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::sync::Arc;

use crate::storage::asset::{Asset, AssetStore};
use crate::storage::blob::BlobStore;
use crate::storage::content::{StoredContent, StoredPayload};
use crate::storage::ids::AssetId;

/// Type-erased storage coordinator using trait objects.
///
/// This version can be passed around without generic parameters, making it
/// easier to integrate into existing code that doesn't want to be generic
/// over storage implementations.
pub struct DynStorageCoordinator {
    blob_store: Arc<dyn BlobStore>,
    asset_store: Arc<dyn AssetStore>,
}

impl DynStorageCoordinator {
    /// Create a new storage coordinator with trait objects
    pub fn new(blob_store: Arc<dyn BlobStore>, asset_store: Arc<dyn AssetStore>) -> Self {
        Self {
            blob_store,
            asset_store,
        }
    }

    /// Extract inline binary content from a payload, store in blob/asset storage,
    /// and return a new payload with AssetRefs replacing inline content.
    pub async fn externalize_assets(&self, payload: StoredPayload) -> Result<StoredPayload> {
        let mut new_content = Vec::with_capacity(payload.content.len());

        for content in payload.content {
            let converted = self.externalize_content(content).await?;
            new_content.push(converted);
        }

        Ok(StoredPayload::new(new_content))
    }

    /// Convert a single content block, externalizing binary data if present
    async fn externalize_content(&self, content: StoredContent) -> Result<StoredContent> {
        match content {
            StoredContent::Image { data, mime_type } => {
                let asset_id = self.store_base64_asset(&data, &mime_type).await?;
                Ok(StoredContent::asset_ref(asset_id, mime_type, None))
            }
            StoredContent::Audio { data, mime_type } => {
                let asset_id = self.store_base64_asset(&data, &mime_type).await?;
                Ok(StoredContent::asset_ref(asset_id, mime_type, None))
            }
            // Pass through other content types unchanged
            other => Ok(other),
        }
    }

    /// Decode base64 data, store in blob storage, register in asset storage,
    /// and return the asset ID (hash).
    async fn store_base64_asset(&self, base64_data: &str, mime_type: &str) -> Result<String> {
        // Decode base64
        let bytes = STANDARD.decode(base64_data)?;
        let size = bytes.len() as i64;

        // Store in blob storage (returns hash)
        let stored_blob = self.blob_store.store(&bytes).await?;

        // Register metadata in asset storage
        let asset_id = AssetId::from_string(&stored_blob.hash);
        let asset = Asset::new(mime_type, size);
        self.asset_store.store(asset_id, asset).await?;

        Ok(stored_blob.hash)
    }

    /// Get access to the blob store for asset resolution
    pub fn blob_store(&self) -> Arc<dyn BlobStore> {
        Arc::clone(&self.blob_store)
    }

    /// Get access to the asset store
    pub fn asset_store(&self) -> Arc<dyn AssetStore> {
        Arc::clone(&self.asset_store)
    }
}

/// Coordinates storage across blob, asset metadata, and session stores.
///
/// This generic version is useful when you have concrete types and want
/// to avoid dynamic dispatch overhead.
pub struct StorageCoordinator<B: BlobStore, A: AssetStore> {
    blob_store: Arc<B>,
    asset_store: Arc<A>,
}

impl<B: BlobStore, A: AssetStore> StorageCoordinator<B, A> {
    /// Create a new storage coordinator
    pub fn new(blob_store: Arc<B>, asset_store: Arc<A>) -> Self {
        Self {
            blob_store,
            asset_store,
        }
    }

    /// Extract inline binary content from a payload, store in blob/asset storage,
    /// and return a new payload with AssetRefs replacing inline content.
    ///
    /// This should be called before persisting messages to convert inline
    /// images/audio to asset references.
    pub async fn externalize_assets(&self, payload: StoredPayload) -> Result<StoredPayload> {
        let mut new_content = Vec::with_capacity(payload.content.len());

        for content in payload.content {
            let converted = self.externalize_content(content).await?;
            new_content.push(converted);
        }

        Ok(StoredPayload::new(new_content))
    }

    /// Convert a single content block, externalizing binary data if present
    async fn externalize_content(&self, content: StoredContent) -> Result<StoredContent> {
        match content {
            StoredContent::Image { data, mime_type } => {
                let asset_id = self.store_base64_asset(&data, &mime_type).await?;
                Ok(StoredContent::asset_ref(asset_id, mime_type, None))
            }
            StoredContent::Audio { data, mime_type } => {
                let asset_id = self.store_base64_asset(&data, &mime_type).await?;
                Ok(StoredContent::asset_ref(asset_id, mime_type, None))
            }
            // Pass through other content types unchanged
            other => Ok(other),
        }
    }

    /// Decode base64 data, store in blob storage, register in asset storage,
    /// and return the asset ID (hash).
    async fn store_base64_asset(&self, base64_data: &str, mime_type: &str) -> Result<String> {
        // Decode base64
        let bytes = STANDARD.decode(base64_data)?;
        let size = bytes.len() as i64;

        // Store in blob storage (returns hash)
        let stored_blob = self.blob_store.store(&bytes).await?;

        // Register metadata in asset storage
        let asset_id = AssetId::from_string(&stored_blob.hash);
        let asset = Asset::new(mime_type, size);
        self.asset_store.store(asset_id, asset).await?;

        Ok(stored_blob.hash)
    }

    /// Get access to the blob store for asset resolution
    pub fn blob_store(&self) -> &Arc<B> {
        &self.blob_store
    }

    /// Get access to the asset store
    pub fn asset_store(&self) -> &Arc<A> {
        &self.asset_store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::blob::StoredBlob;
    use crate::storage::asset::{AssetStoreResult, StoredAsset};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // Mock blob store for testing
    struct MockBlobStore {
        blobs: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl MockBlobStore {
        fn new() -> Self {
            Self {
                blobs: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl BlobStore for MockBlobStore {
        async fn store(&self, data: &[u8]) -> Result<StoredBlob> {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(data);
            let hash = hex::encode(hasher.finalize());

            let mut blobs = self.blobs.lock().unwrap();
            let is_new = !blobs.contains_key(&hash);
            blobs.insert(hash.clone(), data.to_vec());

            Ok(StoredBlob {
                hash,
                size: data.len(),
                is_new,
            })
        }

        async fn get(&self, hash: &str) -> Result<Vec<u8>> {
            let blobs = self.blobs.lock().unwrap();
            blobs
                .get(hash)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Blob not found"))
        }

        async fn exists(&self, hash: &str) -> bool {
            self.blobs.lock().unwrap().contains_key(hash)
        }

        async fn delete(&self, hash: &str) -> Result<bool> {
            Ok(self.blobs.lock().unwrap().remove(hash).is_some())
        }

        async fn list_all(&self) -> Result<Vec<String>> {
            Ok(self.blobs.lock().unwrap().keys().cloned().collect())
        }
    }

    // Mock asset store for testing
    struct MockAssetStore {
        assets: Mutex<HashMap<String, Asset>>,
    }

    impl MockAssetStore {
        fn new() -> Self {
            Self {
                assets: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl AssetStore for MockAssetStore {
        async fn store(&self, id: AssetId, asset: Asset) -> Result<AssetStoreResult> {
            let mut assets = self.assets.lock().unwrap();
            let is_new = !assets.contains_key(id.as_str());
            assets.insert(id.as_str().to_string(), asset);
            Ok(AssetStoreResult { id, is_new })
        }

        async fn get(&self, id: &AssetId) -> Result<Option<StoredAsset>> {
            let assets = self.assets.lock().unwrap();
            Ok(assets.get(id.as_str()).map(|asset| StoredAsset {
                id: id.clone(),
                asset: asset.clone(),
                created_at: 0,
            }))
        }

        async fn exists(&self, id: &AssetId) -> Result<bool> {
            Ok(self.assets.lock().unwrap().contains_key(id.as_str()))
        }

        async fn delete(&self, id: &AssetId) -> Result<bool> {
            Ok(self.assets.lock().unwrap().remove(id.as_str()).is_some())
        }
    }

    #[tokio::test]
    async fn test_externalize_image() {
        let blob_store = Arc::new(MockBlobStore::new());
        let asset_store = Arc::new(MockAssetStore::new());
        let coordinator = StorageCoordinator::new(blob_store.clone(), asset_store.clone());

        // Create a payload with inline image (base64 encoded "test image data")
        let image_data = STANDARD.encode(b"test image data");
        let payload = StoredPayload::new(vec![
            StoredContent::Text { text: "Check this image:".to_string() },
            StoredContent::Image {
                data: image_data,
                mime_type: "image/png".to_string(),
            },
        ]);

        let result = coordinator.externalize_assets(payload).await.unwrap();

        // First content should be unchanged text
        assert!(matches!(&result.content[0], StoredContent::Text { text } if text == "Check this image:"));

        // Second content should now be an AssetRef
        match &result.content[1] {
            StoredContent::AssetRef { asset_id, mime_type, .. } => {
                assert_eq!(mime_type, "image/png");
                // Verify blob was stored
                assert!(blob_store.exists(asset_id).await);
                // Verify asset metadata was stored
                assert!(asset_store.exists(&AssetId::from_string(asset_id)).await.unwrap());
            }
            other => panic!("Expected AssetRef, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_externalize_audio() {
        let blob_store = Arc::new(MockBlobStore::new());
        let asset_store = Arc::new(MockAssetStore::new());
        let coordinator = StorageCoordinator::new(blob_store.clone(), asset_store.clone());

        let audio_data = STANDARD.encode(b"fake audio bytes");
        let payload = StoredPayload::new(vec![StoredContent::Audio {
            data: audio_data,
            mime_type: "audio/mp3".to_string(),
        }]);

        let result = coordinator.externalize_assets(payload).await.unwrap();

        match &result.content[0] {
            StoredContent::AssetRef { mime_type, .. } => {
                assert_eq!(mime_type, "audio/mp3");
            }
            other => panic!("Expected AssetRef, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_text_unchanged() {
        let blob_store = Arc::new(MockBlobStore::new());
        let asset_store = Arc::new(MockAssetStore::new());
        let coordinator = StorageCoordinator::new(blob_store, asset_store);

        let payload = StoredPayload::new(vec![
            StoredContent::Text { text: "Hello world".to_string() },
        ]);

        let result = coordinator.externalize_assets(payload).await.unwrap();

        assert!(matches!(&result.content[0], StoredContent::Text { text } if text == "Hello world"));
    }
}
