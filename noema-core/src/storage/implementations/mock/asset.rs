//! Mock asset store for testing

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use crate::storage::ids::AssetId;
use crate::storage::traits::AssetStore;
use crate::storage::types::{Asset, Stored, stored};

/// Mock asset store with in-memory storage
pub struct MockAssetStore {
    assets: Mutex<HashMap<String, Asset>>,
}

impl MockAssetStore {
    pub fn new() -> Self {
        Self {
            assets: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MockAssetStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AssetStore for MockAssetStore {
    async fn create_asset(&self, asset: Asset) -> Result<AssetId> {
        let mut assets = self.assets.lock().unwrap();
        let id = AssetId::from_string(Uuid::new_v4().to_string());
        assets.insert(id.as_str().to_string(), asset);
        Ok(id)
    }

    async fn get(&self, id: &AssetId) -> Result<Option<Stored<AssetId, Asset>>> {
        let assets = self.assets.lock().unwrap();
        Ok(assets.get(id.as_str()).map(|asset| stored(
            id.clone(),
            asset.clone(),
            0,
        )))
    }

    async fn exists(&self, id: &AssetId) -> Result<bool> {
        Ok(self.assets.lock().unwrap().contains_key(id.as_str()))
    }

    async fn delete(&self, id: &AssetId) -> Result<bool> {
        Ok(self.assets.lock().unwrap().remove(id.as_str()).is_some())
    }
}
