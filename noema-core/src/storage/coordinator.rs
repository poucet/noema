//! Storage coordinator - orchestrates content, blob, and asset storage
//!
//! The coordinator provides a unified interface for converting LLM content
//! to stored references. It handles:
//! - Text → stored in content_blocks → TextRef
//! - Inline images/audio → stored in blob/assets → AssetRef
//! - DocumentRef, ToolCall, ToolResult → pass through
//!
//! It also implements `ContentResolver` for converting refs back to content.

use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use llm::ContentBlock;
use std::sync::Arc;

use crate::storage::content::{ContentResolver, StoredContent};
use crate::storage::ids::{AssetId, ContentBlockId};
use crate::storage::traits::{AssetStore, BlobStore, ContentBlockStore};
use crate::storage::types::{Asset, ContentBlock as ContentBlockData, ContentOrigin, OriginKind};

/// Type-erased storage coordinator using trait objects.
///
/// This version can be passed around without generic parameters, making it
/// easier to integrate into existing code that doesn't want to be generic
/// over storage implementations.
pub struct DynStorageCoordinator {
    blob_store: Arc<dyn BlobStore>,
    asset_store: Arc<dyn AssetStore>,
    content_block_store: Arc<dyn ContentBlockStore>,
}

impl DynStorageCoordinator {
    /// Create a new storage coordinator with trait objects
    pub fn new(
        blob_store: Arc<dyn BlobStore>,
        asset_store: Arc<dyn AssetStore>,
        content_block_store: Arc<dyn ContentBlockStore>,
    ) -> Self {
        Self {
            blob_store,
            asset_store,
            content_block_store,
        }
    }

    /// Convert LLM ContentBlocks to StoredContent refs
    ///
    /// - Text is stored in content_blocks and converted to TextRef
    /// - Inline images/audio are stored in blob/assets and converted to AssetRef
    /// - DocumentRef, ToolCall, ToolResult pass through
    pub async fn store_content(
        &self,
        blocks: Vec<ContentBlock>,
        origin: OriginKind,
    ) -> Result<Vec<StoredContent>> {
        let mut stored = Vec::with_capacity(blocks.len());

        for block in blocks {
            let content = self.store_content_block(block, origin).await?;
            stored.push(content);
        }

        Ok(stored)
    }

    /// Store a single ContentBlock and return its StoredContent reference
    async fn store_content_block(
        &self,
        block: ContentBlock,
        origin: OriginKind,
    ) -> Result<StoredContent> {
        match block {
            ContentBlock::Text { text } => {
                // Store text in content_blocks
                let content_origin = ContentOrigin { kind: Some(origin), ..Default::default() };
                let content_block = ContentBlockData::plain(&text).with_origin(content_origin);
                let result = self.content_block_store.store(content_block).await?;
                Ok(StoredContent::text_ref(result.id))
            }
            ContentBlock::Image { data, mime_type } => {
                let asset_id = self.store_base64_asset(&data, &mime_type).await?;
                Ok(StoredContent::asset_ref(asset_id, mime_type, None))
            }
            ContentBlock::Audio { data, mime_type } => {
                let asset_id = self.store_base64_asset(&data, &mime_type).await?;
                Ok(StoredContent::asset_ref(asset_id, mime_type, None))
            }
            ContentBlock::DocumentRef { id, .. } => Ok(StoredContent::document_ref(id)),
            ContentBlock::ToolCall(call) => Ok(StoredContent::ToolCall(call)),
            ContentBlock::ToolResult(result) => Ok(StoredContent::ToolResult(result)),
        }
    }

    /// Decode base64 data, store in blob storage, register in asset storage,
    /// and return the asset ID.
    async fn store_base64_asset(&self, base64_data: &str, mime_type: &str) -> Result<AssetId> {
        // Decode base64
        let bytes = STANDARD.decode(base64_data)?;
        let size = bytes.len() as i64;

        // Store in blob storage (returns hash)
        let stored_blob = self.blob_store.store(&bytes).await?;

        // Register metadata in asset storage with blob_hash reference
        let asset = Asset::new(&stored_blob.hash, mime_type, size);
        self.asset_store.create_asset(asset).await
    }

    /// Get access to the blob store for asset resolution
    pub fn blob_store(&self) -> Arc<dyn BlobStore> {
        Arc::clone(&self.blob_store)
    }

    /// Get access to the asset store
    pub fn asset_store(&self) -> Arc<dyn AssetStore> {
        Arc::clone(&self.asset_store)
    }

    /// Get access to the content block store
    pub fn content_block_store(&self) -> Arc<dyn ContentBlockStore> {
        Arc::clone(&self.content_block_store)
    }
}

/// Implement ContentResolver to allow resolving StoredContent back to ContentBlock
#[async_trait]
impl ContentResolver for DynStorageCoordinator {
    async fn get_text(&self, id: &ContentBlockId) -> Result<String> {
        self.content_block_store
            .get_text(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Content block not found: {}", id))
    }

    async fn get_asset(&self, id: &str) -> Result<(Vec<u8>, String)> {
        // Get asset metadata for mime type and blob_hash
        let asset_id = AssetId::from_string(id.to_string());
        let stored_asset = self
            .asset_store
            .get(&asset_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Asset not found: {}", id))?;

        // Get blob data using the blob_hash from the asset
        let data = self.blob_store.get(&stored_asset.asset.blob_hash).await?;

        Ok((data, stored_asset.asset.mime_type))
    }
}

/// Coordinates storage across content blocks, blob, and asset stores.
///
/// This generic version is useful when you have concrete types and want
/// to avoid dynamic dispatch overhead.
pub struct StorageCoordinator<B: BlobStore, A: AssetStore, C: ContentBlockStore> {
    blob_store: Arc<B>,
    asset_store: Arc<A>,
    content_block_store: Arc<C>,
}

impl<B: BlobStore, A: AssetStore, C: ContentBlockStore> StorageCoordinator<B, A, C> {
    /// Create a new storage coordinator
    pub fn new(blob_store: Arc<B>, asset_store: Arc<A>, content_block_store: Arc<C>) -> Self {
        Self {
            blob_store,
            asset_store,
            content_block_store,
        }
    }

    /// Convert LLM ContentBlocks to StoredContent refs
    ///
    /// - Text is stored in content_blocks and converted to TextRef
    /// - Inline images/audio are stored in blob/assets and converted to AssetRef
    /// - DocumentRef, ToolCall, ToolResult pass through
    pub async fn store_content(
        &self,
        blocks: Vec<ContentBlock>,
        origin: OriginKind,
    ) -> Result<Vec<StoredContent>> {
        let mut stored = Vec::with_capacity(blocks.len());

        for block in blocks {
            let content = self.store_content_block(block, origin).await?;
            stored.push(content);
        }

        Ok(stored)
    }

    /// Store a single ContentBlock and return its StoredContent reference
    async fn store_content_block(
        &self,
        block: ContentBlock,
        origin: OriginKind,
    ) -> Result<StoredContent> {
        match block {
            ContentBlock::Text { text } => {
                let content_origin = ContentOrigin { kind: Some(origin), ..Default::default() };
                let content_block = ContentBlockData::plain(&text).with_origin(content_origin);
                let result = self.content_block_store.store(content_block).await?;
                Ok(StoredContent::text_ref(result.id))
            }
            ContentBlock::Image { data, mime_type } => {
                let asset_id = self.store_base64_asset(&data, &mime_type).await?;
                Ok(StoredContent::asset_ref(asset_id, mime_type, None))
            }
            ContentBlock::Audio { data, mime_type } => {
                let asset_id = self.store_base64_asset(&data, &mime_type).await?;
                Ok(StoredContent::asset_ref(asset_id, mime_type, None))
            }
            ContentBlock::DocumentRef { id, .. } => Ok(StoredContent::document_ref(id)),
            ContentBlock::ToolCall(call) => Ok(StoredContent::ToolCall(call)),
            ContentBlock::ToolResult(result) => Ok(StoredContent::ToolResult(result)),
        }
    }

    /// Decode base64 data, store in blob storage, register in asset storage,
    /// and return the asset ID.
    async fn store_base64_asset(&self, base64_data: &str, mime_type: &str) -> Result<String> {
        let bytes = STANDARD.decode(base64_data)?;
        let size = bytes.len() as i64;

        let stored_blob = self.blob_store.store(&bytes).await?;

        let asset = Asset::new(&stored_blob.hash, mime_type, size);
        let asset_id = self.asset_store.create_asset(asset).await?;

        Ok(asset_id.into())
    }

    /// Get access to the blob store for asset resolution
    pub fn blob_store(&self) -> &Arc<B> {
        &self.blob_store
    }

    /// Get access to the asset store
    pub fn asset_store(&self) -> &Arc<A> {
        &self.asset_store
    }

    /// Get access to the content block store
    pub fn content_block_store(&self) -> &Arc<C> {
        &self.content_block_store
    }
}

/// Implement ContentResolver for the generic coordinator
#[async_trait]
impl<B: BlobStore, A: AssetStore, C: ContentBlockStore> ContentResolver
    for StorageCoordinator<B, A, C>
{
    async fn get_text(&self, id: &ContentBlockId) -> Result<String> {
        self.content_block_store
            .get_text(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Content block not found: {}", id))
    }

    async fn get_asset(&self, id: &str) -> Result<(Vec<u8>, String)> {
        let asset_id = AssetId::from_string(id.to_string());
        let stored_asset = self
            .asset_store
            .get(&asset_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Asset not found: {}", id))?;

        // Use blob_hash to fetch from blob store
        let data = self.blob_store.get(&stored_asset.asset.blob_hash).await?;

        Ok((data, stored_asset.asset.mime_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::types::{StoredAsset, StoredBlob, StoreResult, StoredContentBlock};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

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
        async fn create_asset(&self, asset: Asset) -> Result<AssetId> {
            let mut assets = self.assets.lock().unwrap();
            let id = AssetId::from_string(Uuid::new_v4().to_string());
            assets.insert(id.as_str().to_string(), asset);
            Ok(id)
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

    // Mock content block store for testing
    struct MockContentBlockStore {
        blocks: Mutex<HashMap<String, String>>,
        counter: Mutex<u64>,
    }

    impl MockContentBlockStore {
        fn new() -> Self {
            Self {
                blocks: Mutex::new(HashMap::new()),
                counter: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl ContentBlockStore for MockContentBlockStore {
        async fn store(&self, block: ContentBlockData) -> Result<StoreResult> {
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
                content: ContentBlockData::plain(text),
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

    #[tokio::test]
    async fn test_store_text_content() {
        let blob_store = Arc::new(MockBlobStore::new());
        let asset_store = Arc::new(MockAssetStore::new());
        let content_block_store = Arc::new(MockContentBlockStore::new());
        let coordinator =
            StorageCoordinator::new(blob_store, asset_store, content_block_store.clone());

        let blocks = vec![ContentBlock::Text {
            text: "Hello world".to_string(),
        }];

        let result = coordinator
            .store_content(blocks, OriginKind::User)
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        match &result[0] {
            StoredContent::TextRef { content_block_id } => {
                // Verify the text was stored
                let stored_text = content_block_store.get_text(content_block_id).await.unwrap();
                assert_eq!(stored_text, Some("Hello world".to_string()));
            }
            other => panic!("Expected TextRef, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_store_image_content() {
        let blob_store = Arc::new(MockBlobStore::new());
        let asset_store = Arc::new(MockAssetStore::new());
        let content_block_store = Arc::new(MockContentBlockStore::new());
        let coordinator = StorageCoordinator::new(
            blob_store.clone(),
            asset_store.clone(),
            content_block_store,
        );

        let image_data = STANDARD.encode(b"fake image bytes");
        let blocks = vec![ContentBlock::Image {
            data: image_data,
            mime_type: "image/png".to_string(),
        }];

        let result = coordinator
            .store_content(blocks, OriginKind::User)
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        match &result[0] {
            StoredContent::AssetRef {
                asset_id,
                mime_type,
                ..
            } => {
                assert_eq!(mime_type, "image/png");
                // Verify blob was stored
                assert!(blob_store.exists(asset_id).await);
                // Verify asset metadata was stored
                assert!(asset_store
                    .exists(&AssetId::from_string(asset_id))
                    .await
                    .unwrap());
            }
            other => panic!("Expected AssetRef, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_resolve_text() {
        let blob_store = Arc::new(MockBlobStore::new());
        let asset_store = Arc::new(MockAssetStore::new());
        let content_block_store = Arc::new(MockContentBlockStore::new());
        let coordinator = StorageCoordinator::new(
            blob_store,
            asset_store,
            content_block_store.clone(),
        );

        // Store some text
        let blocks = vec![ContentBlock::Text {
            text: "Test text".to_string(),
        }];
        let stored = coordinator
            .store_content(blocks, OriginKind::User)
            .await
            .unwrap();

        // Resolve it back
        let resolved = stored[0].resolve(&coordinator).await.unwrap();

        match resolved {
            ContentBlock::Text { text } => {
                assert_eq!(text, "Test text");
            }
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_resolve_image() {
        let blob_store = Arc::new(MockBlobStore::new());
        let asset_store = Arc::new(MockAssetStore::new());
        let content_block_store = Arc::new(MockContentBlockStore::new());
        let coordinator = StorageCoordinator::new(
            blob_store.clone(),
            asset_store.clone(),
            content_block_store,
        );

        let original_data = b"fake image bytes";
        let image_data = STANDARD.encode(original_data);
        let blocks = vec![ContentBlock::Image {
            data: image_data,
            mime_type: "image/png".to_string(),
        }];

        let stored = coordinator
            .store_content(blocks, OriginKind::User)
            .await
            .unwrap();

        // Resolve it back
        let resolved = stored[0].resolve(&coordinator).await.unwrap();

        match resolved {
            ContentBlock::Image { data, mime_type } => {
                assert_eq!(mime_type, "image/png");
                let decoded = STANDARD.decode(&data).unwrap();
                assert_eq!(decoded, original_data);
            }
            other => panic!("Expected Image, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_tool_call_passthrough() {
        let blob_store = Arc::new(MockBlobStore::new());
        let asset_store = Arc::new(MockAssetStore::new());
        let content_block_store = Arc::new(MockContentBlockStore::new());
        let coordinator = StorageCoordinator::new(blob_store, asset_store, content_block_store);

        let tool_call = llm::ToolCall {
            id: "call-1".to_string(),
            name: "test_tool".to_string(),
            arguments: serde_json::json!({"key": "value"}),
        };
        let blocks = vec![ContentBlock::ToolCall(tool_call.clone())];

        let result = coordinator
            .store_content(blocks, OriginKind::Assistant)
            .await
            .unwrap();

        match &result[0] {
            StoredContent::ToolCall(stored_call) => {
                assert_eq!(stored_call.name, "test_tool");
            }
            other => panic!("Expected ToolCall, got {:?}", other),
        }
    }
}
