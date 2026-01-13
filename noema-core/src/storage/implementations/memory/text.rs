//! In-memory TextStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::ids::ContentBlockId;
use crate::storage::traits::{StoredTextBlock, TextStore};
use crate::storage::types::{stored, ContentBlock, ContentHash, Hashed, Keyed, StoreResult};

/// In-memory content block store for testing
#[derive(Debug, Default)]
pub struct MemoryTextStore {
    blocks: Mutex<HashMap<ContentBlockId, StoredTextBlock>>,
    hash_index: Mutex<HashMap<ContentHash, ContentBlockId>>,
}

impl MemoryTextStore {
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
impl TextStore for MemoryTextStore {
    async fn store(&self, content: ContentBlock) -> Result<StoreResult> {
        let hash = ContentHash::from_text(&content.text);

        // Check for existing by hash
        {
            let hash_index = self.hash_index.lock().unwrap();
            if let Some(existing_id) = hash_index.get(&hash) {
                return Ok(Keyed::new(existing_id.clone(), hash));
            }
        }

        // Create new
        let id = ContentBlockId::new();
        let stored_block = stored(
            id.clone(),
            Hashed::new(hash.as_str(), content),
            Self::now(),
        );

        {
            let mut blocks = self.blocks.lock().unwrap();
            let mut hash_index = self.hash_index.lock().unwrap();
            blocks.insert(id.clone(), stored_block);
            hash_index.insert(hash.clone(), id.clone());
        }

        Ok(Keyed::new(id, hash))
    }

    async fn get(&self, id: &ContentBlockId) -> Result<Option<StoredTextBlock>> {
        let blocks = self.blocks.lock().unwrap();
        Ok(blocks.get(id).cloned())
    }

    async fn get_text(&self, id: &ContentBlockId) -> Result<Option<String>> {
        let blocks = self.blocks.lock().unwrap();
        Ok(blocks.get(id).map(|b| b.text().to_string()))
    }

    async fn exists(&self, id: &ContentBlockId) -> Result<bool> {
        Ok(self.blocks.lock().unwrap().contains_key(id))
    }

    async fn find_by_hash(&self, hash: &str) -> Result<Option<ContentBlockId>> {
        let hash_index = self.hash_index.lock().unwrap();
        Ok(hash_index.get(&ContentHash::from_string(hash)).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_get() {
        let store = MemoryTextStore::new();
        let content = ContentBlock::plain("Hello, world!");

        let result = store.store(content).await.unwrap();

        let stored = store.get(&result.id).await.unwrap().unwrap();
        assert_eq!(stored.text(), "Hello, world!");
    }

    #[tokio::test]
    async fn test_deduplication() {
        let store = MemoryTextStore::new();

        let first = store.store(ContentBlock::plain("same")).await.unwrap();

        let second = store.store(ContentBlock::plain("same")).await.unwrap();
        assert_eq!(first.id.as_str(), second.id.as_str());
    }

    #[tokio::test]
    async fn test_get_text() {
        let store = MemoryTextStore::new();
        let content = ContentBlock::plain("test text");

        let result = store.store(content).await.unwrap();
        let text = store.get_text(&result.id).await.unwrap();
        assert_eq!(text, Some("test text".to_string()));
    }

    #[tokio::test]
    async fn test_find_by_hash() {
        let store = MemoryTextStore::new();
        let content = ContentBlock::plain("findme");

        let result = store.store(content).await.unwrap();
        let found = store.find_by_hash(result.content.as_str()).await.unwrap();
        assert_eq!(found.map(|id| id.as_str().to_string()), Some(result.id.as_str().to_string()));

        let not_found = store.find_by_hash("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }
}
