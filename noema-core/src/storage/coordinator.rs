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
use crate::storage::ids::{AssetId, ContentBlockId, ConversationId, SpanId, ViewId};
use crate::storage::session::{ResolvedContent, ResolvedMessage};
use crate::storage::traits::{AssetStore, BlobStore, ContentBlockStore, TurnStore};
use crate::storage::types::{
    Asset, ContentBlock as ContentBlockData, ContentOrigin, MessageInfo, MessageRole, OriginKind,
    TurnWithContent,
};

/// Coordinates storage across all store types.
///
/// This generic version is useful when you have concrete types and want
/// to avoid dynamic dispatch overhead.
pub struct StorageCoordinator<B: BlobStore, A: AssetStore, C: ContentBlockStore, T: TurnStore> {
    blob_store: Arc<B>,
    asset_store: Arc<A>,
    content_block_store: Arc<C>,
    turn_store: Arc<T>,
}

impl<B: BlobStore, A: AssetStore, C: ContentBlockStore, T: TurnStore> StorageCoordinator<B, A, C, T> {
    /// Create a new storage coordinator
    pub fn new(blob_store: Arc<B>, asset_store: Arc<A>, content_block_store: Arc<C>, turn_store: Arc<T>) -> Self {
        Self {
            blob_store,
            asset_store,
            content_block_store,
            turn_store,
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
    async fn store_base64_asset(&self, base64_data: &str, mime_type: &str) -> Result<AssetId> {
        let bytes = STANDARD.decode(base64_data)?;
        self.store_asset(&bytes, mime_type, None).await
    }

    /// Store raw bytes as an asset with optional filename.
    ///
    /// Stores the data in blob storage and registers metadata in the asset store.
    /// Returns the asset ID for referencing.
    pub async fn store_asset(
        &self,
        data: &[u8],
        mime_type: &str,
        filename: Option<String>,
    ) -> Result<AssetId> {
        let stored_blob = self.blob_store.store(data).await?;

        let mut asset = Asset::new(&stored_blob.hash, mime_type, data.len() as i64);
        if let Some(name) = filename {
            asset = asset.with_filename(name);
        }

        self.asset_store.create_asset(asset).await
    }

    /// Get blob data by hash (for asset protocol handler)
    pub async fn get_blob(&self, hash: &str) -> Result<Vec<u8>> {
        self.blob_store.get(hash).await
    }

    /// Get access to the content block store
    pub fn content_block_store(&self) -> &Arc<C> {
        &self.content_block_store
    }

    /// Get access to the turn store
    pub fn turn_store(&self) -> &Arc<T> {
        &self.turn_store
    }

    /// Open a session for a conversation, resolving or creating the main view.
    ///
    /// This method handles the multi-store coordination of:
    /// 1. Getting or creating the main view for the conversation
    /// 2. Loading the view path (turns with content)
    /// 3. Resolving stored content to resolved messages
    ///
    /// Returns (view_id, resolved_messages) for Session construction.
    pub async fn open_session(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<(ViewId, Vec<ResolvedMessage>)> {
        // Get or create main view
        let view_id = match self.turn_store.get_main_view(conversation_id).await? {
            Some(v) => v.id,
            None => {
                self.turn_store
                    .create_view(conversation_id, Some("main"), true)
                    .await?
                    .id
            }
        };

        // Load view path and resolve content
        let path = self.turn_store.get_view_path(&view_id).await?;
        let resolved_cache = self.resolve_path(&path).await?;

        Ok((view_id, resolved_cache))
    }

    /// Resolve a view path (turns with content) to resolved messages.
    async fn resolve_path(&self, path: &[TurnWithContent]) -> Result<Vec<ResolvedMessage>> {
        let mut messages = Vec::new();

        for turn in path {
            for msg in &turn.messages {
                let content_refs: Vec<StoredContent> =
                    msg.content.iter().map(|c| c.content.clone()).collect();

                let resolved = self.resolve_stored_content(&content_refs).await?;
                messages.push(ResolvedMessage::new(msg.message.role, resolved));
            }
        }

        Ok(messages)
    }

    /// Resolve stored content references to resolved content.
    pub async fn resolve_stored_content(
        &self,
        content: &[StoredContent],
    ) -> Result<Vec<ResolvedContent>> {
        let mut resolved = Vec::with_capacity(content.len());

        for item in content {
            let r = match item {
                StoredContent::TextRef { content_block_id } => {
                    let text = self.content_block_store.require_text(content_block_id).await?;
                    ResolvedContent::text(text)
                }
                StoredContent::AssetRef {
                    asset_id,
                    mime_type,
                    filename,
                } => ResolvedContent::asset(asset_id.clone(), mime_type, filename.clone()),
                StoredContent::DocumentRef { document_id } => {
                    ResolvedContent::document(document_id)
                }
                StoredContent::ToolCall(call) => ResolvedContent::tool_call(call.clone()),
                StoredContent::ToolResult(result) => ResolvedContent::tool_result(result.clone()),
            };
            resolved.push(r);
        }

        Ok(resolved)
    }

    /// Store a message and add it to a span, returning the resolved content.
    ///
    /// This coordinates storing content blocks, adding the message to the turn store,
    /// and resolving the content for caching.
    pub async fn store_message(
        &self,
        span_id: &SpanId,
        role: MessageRole,
        content: Vec<ContentBlock>,
        origin: OriginKind,
    ) -> Result<(MessageInfo, Vec<ResolvedContent>)> {
        // Store content blocks
        let stored = self.store_content(content, origin).await?;

        // Add message to turn store
        let message_info = self.turn_store.add_message(span_id, role, &stored).await?;

        // Resolve for caching
        let resolved = self.resolve_stored_content(&stored).await?;

        Ok((message_info, resolved))
    }
}

/// Implement ContentResolver for the generic coordinator
#[async_trait]
impl<B: BlobStore, A: AssetStore, C: ContentBlockStore, T: TurnStore> ContentResolver
    for StorageCoordinator<B, A, C, T>
{
    async fn get_text(&self, id: &ContentBlockId) -> Result<String> {
        self.content_block_store
            .get_text(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Content block not found: {}", id))
    }

    async fn get_asset(&self, id: &AssetId) -> Result<(Vec<u8>, String)> {
        let stored_asset = self
            .asset_store
            .get(id)
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
    use crate::storage::content::StoredContent;
    use crate::storage::ids::{ConversationId, MessageId, SpanId, TurnId, ViewId};
    use crate::storage::types::{
        MessageInfo, MessageRole, MessageWithContent, SpanInfo, SpanRole, StoredAsset,
        StoredBlob, StoreResult, StoredContentBlock, TurnInfo, TurnWithContent, ViewInfo,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    // Mock turn store (not used in these tests, just needs to exist)
    struct MockTurnStore;

    #[async_trait]
    impl TurnStore for MockTurnStore {
        async fn add_turn(&self, _: &ConversationId, _: SpanRole) -> Result<TurnInfo> {
            unimplemented!()
        }
        async fn get_turns(&self, _: &ConversationId) -> Result<Vec<TurnInfo>> {
            unimplemented!()
        }
        async fn get_turn(&self, _: &TurnId) -> Result<Option<TurnInfo>> {
            unimplemented!()
        }
        async fn add_span(&self, _: &TurnId, _: Option<&str>) -> Result<SpanInfo> {
            unimplemented!()
        }
        async fn get_spans(&self, _: &TurnId) -> Result<Vec<SpanInfo>> {
            unimplemented!()
        }
        async fn get_span(&self, _: &SpanId) -> Result<Option<SpanInfo>> {
            unimplemented!()
        }
        async fn add_message(&self, _: &SpanId, _: MessageRole, _: &[StoredContent]) -> Result<MessageInfo> {
            unimplemented!()
        }
        async fn get_messages(&self, _: &SpanId) -> Result<Vec<MessageInfo>> {
            unimplemented!()
        }
        async fn get_messages_with_content(&self, _: &SpanId) -> Result<Vec<MessageWithContent>> {
            unimplemented!()
        }
        async fn get_message(&self, _: &MessageId) -> Result<Option<MessageInfo>> {
            unimplemented!()
        }
        async fn create_view(&self, _: &ConversationId, _: Option<&str>, _: bool) -> Result<ViewInfo> {
            unimplemented!()
        }
        async fn get_main_view(&self, _: &ConversationId) -> Result<Option<ViewInfo>> {
            unimplemented!()
        }
        async fn get_views(&self, _: &ConversationId) -> Result<Vec<ViewInfo>> {
            unimplemented!()
        }
        async fn select_span(&self, _: &ViewId, _: &TurnId, _: &SpanId) -> Result<()> {
            unimplemented!()
        }
        async fn get_selected_span(&self, _: &ViewId, _: &TurnId) -> Result<Option<SpanId>> {
            unimplemented!()
        }
        async fn get_view_path(&self, _: &ViewId) -> Result<Vec<TurnWithContent>> {
            unimplemented!()
        }
        async fn fork_view(&self, _: &ViewId, _: &TurnId, _: Option<&str>) -> Result<ViewInfo> {
            unimplemented!()
        }
        async fn fork_view_with_selections(&self, _: &ViewId, _: &TurnId, _: Option<&str>, _: &[(TurnId, SpanId)]) -> Result<ViewInfo> {
            unimplemented!()
        }
        async fn get_view_context_at(&self, _: &ViewId, _: &TurnId) -> Result<Vec<TurnWithContent>> {
            unimplemented!()
        }
        async fn edit_turn(&self, _: &ViewId, _: &TurnId, _: Vec<(MessageRole, Vec<StoredContent>)>, _: Option<&str>, _: bool, _: Option<&str>) -> Result<(SpanInfo, Option<ViewInfo>)> {
            unimplemented!()
        }
        async fn add_user_turn(&self, _: &ConversationId, _: &str) -> Result<(TurnInfo, SpanInfo, MessageInfo)> {
            unimplemented!()
        }
        async fn add_assistant_turn(&self, _: &ConversationId, _: &str, _: &str) -> Result<(TurnInfo, SpanInfo, MessageInfo)> {
            unimplemented!()
        }
    }

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
        let turn_store = Arc::new(MockTurnStore);
        let coordinator =
            StorageCoordinator::new(blob_store, asset_store, content_block_store.clone(), turn_store);

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
        let turn_store = Arc::new(MockTurnStore);
        let coordinator = StorageCoordinator::new(
            blob_store.clone(),
            asset_store.clone(),
            content_block_store,
            turn_store,
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
        let turn_store = Arc::new(MockTurnStore);
        let coordinator = StorageCoordinator::new(
            blob_store,
            asset_store,
            content_block_store.clone(),
            turn_store,
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
        let turn_store = Arc::new(MockTurnStore);
        let coordinator = StorageCoordinator::new(
            blob_store.clone(),
            asset_store.clone(),
            content_block_store,
            turn_store,
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
        let turn_store = Arc::new(MockTurnStore);
        let coordinator = StorageCoordinator::new(blob_store, asset_store, content_block_store, turn_store);

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
