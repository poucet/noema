//! Mock text store for testing

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::ContentBlockId;
use crate::storage::traits::TextStore;
use crate::storage::types::{ContentBlock, StoredContentBlock, StoreResult};

/// Mock text store with in-memory storage
pub struct MockTextStore {
    blocks: Mutex<HashMap<String, String>>,
    counter: Mutex<u64>,
}

impl MockTextStore {
    pub fn new() -> Self {
        Self {
            blocks: Mutex::new(HashMap::new()),
            counter: Mutex::new(0),
        }
    }
}

impl Default for MockTextStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TextStore for MockTextStore {
    async fn store(&self, block: ContentBlock) -> Result<StoreResult> {
        let mut counter = self.counter.lock().unwrap();
        *counter += 1;
        let id = ContentBlockId::from_string(format!("block-{}", *counter));
        let hash = format!("hash-{}", *counter);

        let mut blocks = self.blocks.lock().unwrap();
        blocks.insert(id.as_str().to_string(), block.text);

        Ok(StoreResult {
            id,
            hash,
            is_new: true,
        })
    }

    async fn get(&self, id: &ContentBlockId) -> Result<Option<StoredContentBlock>> {
        let blocks = self.blocks.lock().unwrap();
        Ok(blocks.get(id.as_str()).map(|text| StoredContentBlock {
            id: id.clone(),
            content_hash: "hash".to_string(),
            content: ContentBlock::plain(text),
            created_at: 0,
        }))
    }

    async fn get_text(&self, id: &ContentBlockId) -> Result<Option<String>> {
        let blocks = self.blocks.lock().unwrap();
        Ok(blocks.get(id.as_str()).cloned())
    }

    async fn exists(&self, id: &ContentBlockId) -> Result<bool> {
        Ok(self.blocks.lock().unwrap().contains_key(id.as_str()))
    }

    async fn find_by_hash(&self, _hash: &str) -> Result<Option<ContentBlockId>> {
        Ok(None)
    }
}
