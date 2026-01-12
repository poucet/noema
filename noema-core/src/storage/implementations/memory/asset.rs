//! In-memory AssetStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::ids::AssetId;
use crate::storage::traits::AssetStore;
use crate::storage::types::{Asset, AssetStoreResult, StoredAsset};

/// In-memory asset store for testing
#[derive(Debug, Default)]
pub struct MemoryAssetStore {
    assets: Mutex<HashMap<String, StoredAsset>>,
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
    async fn store(&self, id: AssetId, asset: Asset) -> Result<AssetStoreResult> {
        let mut assets = self.assets.lock().unwrap();
        let is_new = !assets.contains_key(id.as_str());

        let stored = StoredAsset {
            id: id.clone(),
            asset,
            created_at: Self::now(),
        };
        assets.insert(id.as_str().to_string(), stored);

        Ok(AssetStoreResult { id, is_new })
    }

    async fn get(&self, id: &AssetId) -> Result<Option<StoredAsset>> {
        let assets = self.assets.lock().unwrap();
        Ok(assets.get(id.as_str()).cloned())
    }

    async fn exists(&self, id: &AssetId) -> Result<bool> {
        Ok(self.assets.lock().unwrap().contains_key(id.as_str()))
    }

    async fn delete(&self, id: &AssetId) -> Result<bool> {
        Ok(self.assets.lock().unwrap().remove(id.as_str()).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_get() {
        let store = MemoryAssetStore::new();
        let id = AssetId::from_string("test-asset-123");
        let asset = Asset::new("image/png", 1024);

        let result = store.store(id.clone(), asset.clone()).await.unwrap();
        assert!(result.is_new);
        assert_eq!(result.id.as_str(), id.as_str());

        let stored = store.get(&id).await.unwrap().unwrap();
        assert_eq!(stored.asset.mime_type, "image/png");
        assert_eq!(stored.asset.size_bytes, 1024);
    }

    #[tokio::test]
    async fn test_exists() {
        let store = MemoryAssetStore::new();
        let id = AssetId::from_string("exists-test");

        assert!(!store.exists(&id).await.unwrap());

        store.store(id.clone(), Asset::new("text/plain", 100)).await.unwrap();
        assert!(store.exists(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let store = MemoryAssetStore::new();
        let id = AssetId::from_string("delete-test");

        store.store(id.clone(), Asset::new("text/plain", 50)).await.unwrap();
        assert!(store.exists(&id).await.unwrap());

        assert!(store.delete(&id).await.unwrap());
        assert!(!store.exists(&id).await.unwrap());
        assert!(!store.delete(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_update_existing() {
        let store = MemoryAssetStore::new();
        let id = AssetId::from_string("update-test");

        let first = store.store(id.clone(), Asset::new("image/png", 100)).await.unwrap();
        assert!(first.is_new);

        let second = store.store(id.clone(), Asset::new("image/jpeg", 200)).await.unwrap();
        assert!(!second.is_new);

        let stored = store.get(&id).await.unwrap().unwrap();
        assert_eq!(stored.asset.mime_type, "image/jpeg");
        assert_eq!(stored.asset.size_bytes, 200);
    }
}
