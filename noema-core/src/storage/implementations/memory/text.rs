//! In-memory TextStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::ids::ContentBlockId;
use crate::storage::traits::{StoredTextBlock, TextStore, StoredContentRef};
use crate::storage::types::{stored, ContentBlock, ContentHash, Hashed, Keyed};

/// In-memory content block store for testing
#[derive(Debug, Default)]
pub struct MemoryTextStore {
    blocks: Mutex<HashMap<ContentBlockId, StoredTextBlock>>,
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
    async fn store(&self, content: ContentBlock) -> Result<StoredContentRef> {
        let hash = ContentHash::from_text(&content.text);

        // Note: We intentionally do NOT deduplicate by hash here.
        // Each ContentBlock may have different metadata (origin, content_type, is_private)
        // even if the text content is the same.

        let id = ContentBlockId::new();
        let stored_block = stored(
            id.clone(),
            Hashed::new(hash.as_str(), content),
            Self::now(),
        );

        self.blocks.lock().unwrap().insert(id.clone(), stored_block);

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
    async fn test_no_deduplication() {
        // Each ContentBlock gets its own ID even with same text,
        // because metadata (origin, content_type, is_private) may differ
        let store = MemoryTextStore::new();

        let first = store.store(ContentBlock::plain("same")).await.unwrap();
        let second = store.store(ContentBlock::plain("same")).await.unwrap();

        assert_ne!(first.id.as_str(), second.id.as_str());
        assert_eq!(first.content.as_str(), second.content.as_str()); // same hash
    }

    #[tokio::test]
    async fn test_get_text() {
        let store = MemoryTextStore::new();
        let content = ContentBlock::plain("test text");

        let result = store.store(content).await.unwrap();
        let text = store.get_text(&result.id).await.unwrap();
        assert_eq!(text, Some("test text".to_string()));
    }

}
