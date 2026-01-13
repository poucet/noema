//! In-memory AssetStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

use crate::storage::ids::AssetId;
use crate::storage::traits::AssetStore;
use crate::storage::types::{Asset, Stored};

/// In-memory asset store for testing
#[derive(Debug, Default)]
pub struct MemoryAssetStore {
    assets: Mutex<HashMap<AssetId, Stored<AssetId, Asset>>>,
}

impl MemoryAssetStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }
}

#[async_trait]
impl AssetStore for MemoryAssetStore {
    async fn create_asset(&self, asset: Asset) -> Result<AssetId> {
        let mut assets = self.assets.lock().unwrap();
        let id = AssetId::from_string(Uuid::new_v4().to_string());

        let stored = Stored::new(
            id.clone(),
            asset,
            Self::now(),
        );
        assets.insert(id.clone(), stored);

        Ok(id)
    }

    async fn get(&self, id: &AssetId) -> Result<Option<Stored<AssetId, Asset>>> {
        let assets = self.assets.lock().unwrap();
        Ok(assets.get(id).cloned())
    }

    async fn exists(&self, id: &AssetId) -> Result<bool> {
        Ok(self.assets.lock().unwrap().contains_key(id))
    }

    async fn delete(&self, id: &AssetId) -> Result<bool> {
        Ok(self.assets.lock().unwrap().remove(id).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get() {
        let store = MemoryAssetStore::new();
        let asset = Asset::new("test-blob-hash", "image/png", 1024);

        let id = store.create_asset(asset.clone()).await.unwrap();

        let stored = store.get(&id).await.unwrap().unwrap();
        assert_eq!(stored.asset.mime_type, "image/png");
        assert_eq!(stored.asset.size_bytes, 1024);
        assert_eq!(stored.asset.blob_hash, "test-blob-hash");
    }

    #[tokio::test]
    async fn test_exists() {
        let store = MemoryAssetStore::new();

        let id = store.create_asset(Asset::new("hash", "text/plain", 100)).await.unwrap();
        assert!(store.exists(&id).await.unwrap());

        let fake_id = AssetId::from_string("nonexistent".to_string());
        assert!(!store.exists(&fake_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let store = MemoryAssetStore::new();

        let id = store.create_asset(Asset::new("hash", "text/plain", 50)).await.unwrap();
        assert!(store.exists(&id).await.unwrap());

        assert!(store.delete(&id).await.unwrap());
        assert!(!store.exists(&id).await.unwrap());
        assert!(!store.delete(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_same_blob_different_ids() {
        let store = MemoryAssetStore::new();

        let id1 = store.create_asset(Asset::new("same_hash", "image/png", 100)).await.unwrap();
        let id2 = store.create_asset(Asset::new("same_hash", "image/png", 100)).await.unwrap();

        // Different IDs for same blob
        assert_ne!(id1.as_str(), id2.as_str());

        // Both exist
        assert!(store.exists(&id1).await.unwrap());
        assert!(store.exists(&id2).await.unwrap());
    }
}
