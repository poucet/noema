//! Storage coordinator - orchestrates multi-store operations
//!
//! The coordinator handles operations that require multiple stores working together:
//! - Converting LLM content to stored references (text, assets, documents)
//! - Session management (conversation + turn stores)
//! - Content resolution (text + asset + blob stores)
//!
//! For single-store operations, access stores directly via `Stores` trait.
//! Implements `ContentResolver` and `DocumentResolver` for resolving refs.

use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use llm::ContentBlock;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::storage::content::{ContentResolver, InputContent, StoredContent};
use crate::storage::ids::{AssetId, ContentBlockId, ConversationId, SpanId, TurnId, UserId, ViewId};
use crate::storage::session::{ResolvedContent, ResolvedMessage};
use crate::storage::traits::{
    AssetStore, BlobStore, ConversationStore, StorageTypes, Stores, TextStore, TurnStore,
};
use crate::storage::types::{
    Asset, ContentBlock as ContentBlockData, ContentOrigin, MessageRole, OriginKind,
    TurnWithContent,
};

/// Coordinates storage across all store types.
///
/// Generic over `S: StorageTypes` which bundles all storage type associations.
/// Takes a `Stores<S>` implementation to access individual stores.
pub struct StorageCoordinator<S: StorageTypes> {
    blob_store: Arc<S::Blob>,
    asset_store: Arc<S::Asset>,
    content_block_store: Arc<S::Text>,
    conversation_store: Arc<S::Conversation>,
    turn_store: Arc<S::Turn>,
    _marker: PhantomData<S>,
}

impl<S: StorageTypes> StorageCoordinator<S> {
    /// Create a new storage coordinator from a Stores implementation
    pub fn from_stores(stores: &impl Stores<S>) -> Self {
        Self {
            blob_store: stores.blob(),
            asset_store: stores.asset(),
            content_block_store: stores.text(),
            conversation_store: stores.conversation(),
            turn_store: stores.turn(),
            _marker: PhantomData,
        }
    }

    /// Create a new storage coordinator from individual store instances
    pub fn new(
        blob_store: Arc<S::Blob>,
        asset_store: Arc<S::Asset>,
        content_block_store: Arc<S::Text>,
        conversation_store: Arc<S::Conversation>,
        turn_store: Arc<S::Turn>,
    ) -> Self {
        Self {
            blob_store,
            asset_store,
            content_block_store,
            conversation_store,
            turn_store,
            _marker: PhantomData,
        }
    }

    /// Store a single ContentBlock and return its StoredContent reference
    ///
    /// - Text is stored in content_blocks and converted to TextRef
    /// - Inline images/audio are stored in blob/assets and converted to AssetRef
    /// - DocumentRef, ToolCall, ToolResult pass through
    pub async fn store_content_block(
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
                let asset_id = self.store_asset(&data, &mime_type).await?;
                Ok(StoredContent::asset_ref(asset_id, mime_type))
            }
            ContentBlock::Audio { data, mime_type } => {
                let asset_id = self.store_asset(&data, &mime_type).await?;
                Ok(StoredContent::asset_ref(asset_id, mime_type))
            }
            ContentBlock::DocumentRef { id, .. } => Ok(StoredContent::document_ref(id)),
            ContentBlock::ToolCall(call) => Ok(StoredContent::ToolCall(call)),
            ContentBlock::ToolResult(result) => Ok(StoredContent::ToolResult(result)),
        }
    }

    /// Convert InputContent from UI to StoredContent refs
    ///
    /// - Text is stored in content_blocks → TextRef
    /// - Image/Audio base64 data is stored in blob/assets → AssetRef
    /// - DocumentRef passes through
    /// - AssetRef passes through (already stored)
    pub async fn store_input_content(
        &self,
        content: Vec<InputContent>,
        origin: OriginKind,
    ) -> Result<Vec<StoredContent>> {
        let mut stored = Vec::with_capacity(content.len());

        for item in content {
            let stored_item = match item {
                InputContent::Text { text } => {
                    let content_origin = ContentOrigin { kind: Some(origin), ..Default::default() };
                    let content_block = ContentBlockData::plain(&text).with_origin(content_origin);
                    let result = self.content_block_store.store(content_block).await?;
                    StoredContent::text_ref(result.id)
                }
                InputContent::Image { data, mime_type } => {
                    let asset_id = self.store_asset(&data, &mime_type).await?;
                    StoredContent::asset_ref(asset_id, mime_type)
                }
                InputContent::Audio { data, mime_type } => {
                    let asset_id = self.store_asset(&data, &mime_type).await?;
                    StoredContent::asset_ref(asset_id, mime_type)
                }
                InputContent::DocumentRef { id } => StoredContent::document_ref(id),
                InputContent::AssetRef { asset_id, mime_type } => {
                    StoredContent::asset_ref(asset_id, mime_type)
                }
            };
            stored.push(stored_item);
        }

        Ok(stored)
    }

    /// Decode base64 data, store in blob storage, register in asset storage,
    /// and return the asset ID.
    pub async fn store_asset(&self, base64_data: &str, mime_type: &str) -> Result<AssetId> {
        let bytes = STANDARD.decode(base64_data)?;
        let stored_blob = self.blob_store.store(&bytes).await?;
        let asset = Asset::new(&stored_blob.hash, mime_type, bytes.len() as i64);
        self.asset_store.create_asset(asset).await
    }

    /// Get blob data by hash
    pub async fn get_blob(&self, hash: &str) -> Result<Vec<u8>> {
        self.blob_store.get(hash).await
    }

    // ========== Turn/Span Methods ==========

    /// Create a new turn (without span or selection).
    pub async fn create_turn(&self, role: crate::storage::types::SpanRole) -> Result<TurnId> {
        let turn = self.turn_store.create_turn(role).await?;
        Ok(turn.id)
    }

    /// Create a span at a turn and select it in the view.
    pub async fn create_and_select_span(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
        model_id: Option<&str>,
    ) -> Result<SpanId> {
        let span = self.turn_store.create_span(turn_id, model_id).await?;
        self.turn_store.select_span(view_id, turn_id, &span.id).await?;
        Ok(span.id)
    }

    // ========== Session Methods ==========

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
        // Get conversation to check for main_view_id
        let conv = self.conversation_store
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Conversation not found: {}", conversation_id))?;

        // Conversation has main_view_id - use it
        self.open_session_with_view(&conv.main_view_id).await
            .map(|resolved| (conv.main_view_id, resolved))
    }

    /// Create a new conversation with its main view.
    ///
    /// This method handles the multi-store coordination of:
    /// 1. Creating the conversation record
    /// 2. Creating the main view
    /// 3. Setting the main_view_id on the conversation
    ///
    /// Returns the ConversationId for further operations.
    pub async fn create_conversation_with_view(
        &self,
        user_id: &UserId,
        name: Option<&str>,
    ) -> Result<ConversationId> {
        // Create conversation record
        let conversation_id = self.conversation_store
            .create_conversation(user_id, name)
            .await?;

        // Create main view
        let view = self.turn_store.create_view().await?;

        // Link them
        self.conversation_store
            .set_main_view_id(&conversation_id, &view.id)
            .await?;

        Ok(conversation_id)
    }

    /// Open a session for a specific view.
    ///
    /// Loads the view path and resolves all content for Session construction.
    /// Returns resolved messages for the view.
    pub async fn open_session_with_view(
        &self,
        view_id: &ViewId,
    ) -> Result<Vec<ResolvedMessage>> {
        let path = self.turn_store.get_view_path(view_id).await?;
        self.resolve_path(&path).await
    }

    /// Resolve a view path (turns with content) to resolved messages.
    async fn resolve_path(&self, path: &[TurnWithContent]) -> Result<Vec<ResolvedMessage>> {
        let mut messages = Vec::new();

        for turn in path {
            let turn_id = turn.turn.id.clone();
            for msg in &turn.messages {
                let resolved = self.resolve_stored_content(&msg.content).await?;
                messages.push(ResolvedMessage::new(msg.message.role, resolved, turn_id.clone()));
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
                    let text = self
                        .content_block_store
                        .get_text(content_block_id)
                        .await?
                        .ok_or_else(|| {
                            anyhow::anyhow!("Content block not found: {}", content_block_id)
                        })?;
                    ResolvedContent::text(text)
                }
                StoredContent::AssetRef {
                    asset_id,
                    mime_type,
                } => {
                    // Look up asset to get blob_hash and load data for LLM
                    let stored_asset = self.asset_store.get(asset_id).await?
                        .ok_or_else(|| anyhow::anyhow!("Asset not found: {}", asset_id))?;
                    let blob_hash = stored_asset.asset.blob_hash.clone();

                    // Load blob data and create resolved ContentBlock for LLM
                    let resolved_block = match self.blob_store.get(&blob_hash).await {
                        Ok(data) => {
                            let base64_data = STANDARD.encode(&data);
                            if mime_type.starts_with("image/") {
                                Some(ContentBlock::Image {
                                    data: base64_data,
                                    mime_type: mime_type.clone(),
                                })
                            } else if mime_type.starts_with("audio/") {
                                Some(ContentBlock::Audio {
                                    data: base64_data,
                                    mime_type: mime_type.clone(),
                                })
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    };
                    ResolvedContent::asset(
                        asset_id.clone(),
                        blob_hash,
                        mime_type.clone(),
                        resolved_block,
                    )
                }
                StoredContent::DocumentRef { document_id } => {
                    ResolvedContent::document(document_id.clone())
                }
                StoredContent::ToolCall(call) => ResolvedContent::tool_call(call.clone()),
                StoredContent::ToolResult(result) => ResolvedContent::tool_result(result.clone()),
            };
            resolved.push(r);
        }

        Ok(resolved)
    }

    /// Add a message to a span, returning a resolved message for caching.
    ///
    /// Stores content blocks, adds the message to the span, and resolves
    /// content for display/LLM use.
    pub async fn add_message(
        &self,
        span_id: &SpanId,
        turn_id: &TurnId,
        role: MessageRole,
        content: Vec<ContentBlock>,
        origin: OriginKind,
    ) -> Result<ResolvedMessage> {
        // Store content blocks
        let mut stored = Vec::with_capacity(content.len());
        for block in content {
            stored.push(self.store_content_block(block, origin).await?);
        }

        // Add message to turn store
        self.turn_store.add_message(span_id, role, &stored).await?;

        // Resolve for caching
        let resolved = self.resolve_stored_content(&stored).await?;

        Ok(ResolvedMessage::new(role, resolved, turn_id.clone()))
    }

    /// Get resolved context up to (but not including) a specific turn.
    ///
    /// Used for regeneration - returns messages that should be sent to LLM
    /// before generating a new response at the target turn.
    pub async fn get_context_before_turn(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
    ) -> Result<Vec<ResolvedMessage>> {
        let context_path = self.turn_store
            .get_view_context_at(view_id, turn_id)
            .await?;

        self.resolve_path(&context_path).await
    }
}

/// Implement ContentResolver for the generic coordinator
#[async_trait]
impl<S: StorageTypes> ContentResolver for StorageCoordinator<S> {
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
    use crate::storage::implementations::mock::{
        MockAssetStore, MockBlobStore, MockConversationStore, MockStorage, MockTextStore,
        MockTurnStore,
    };
    use crate::storage::traits::AssetStore;

    fn make_coordinator(content_block_store: Arc<MockTextStore>) -> StorageCoordinator<MockStorage> {
        StorageCoordinator::new(
            Arc::new(MockBlobStore::new()),
            Arc::new(MockAssetStore::new()),
            content_block_store,
            Arc::new(MockConversationStore),
            Arc::new(MockTurnStore),
        )
    }

    fn make_coordinator_with_stores(
        blob_store: Arc<MockBlobStore>,
        asset_store: Arc<MockAssetStore>,
        content_block_store: Arc<MockTextStore>,
    ) -> StorageCoordinator<MockStorage> {
        StorageCoordinator::new(
            blob_store,
            asset_store,
            content_block_store,
            Arc::new(MockConversationStore),
            Arc::new(MockTurnStore),
        )
    }

    #[tokio::test]
    async fn test_store_text_content() {
        let content_block_store = Arc::new(MockTextStore::new());
        let coordinator = make_coordinator(content_block_store.clone());

        let block = ContentBlock::Text {
            text: "Hello world".to_string(),
        };

        let result = coordinator
            .store_content_block(block, OriginKind::User)
            .await
            .unwrap();

        match &result {
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
        let content_block_store = Arc::new(MockTextStore::new());
        let coordinator = make_coordinator_with_stores(
            blob_store.clone(),
            asset_store.clone(),
            content_block_store,
        );

        let image_data = STANDARD.encode(b"fake image bytes");
        let block = ContentBlock::Image {
            data: image_data,
            mime_type: "image/png".to_string(),
        };

        let result = coordinator
            .store_content_block(block, OriginKind::User)
            .await
            .unwrap();

        match &result {
            StoredContent::AssetRef {
                asset_id,
                mime_type,
                ..
            } => {
                assert_eq!(mime_type, "image/png");
                // Verify asset metadata was stored
                assert!(asset_store.exists(asset_id).await.unwrap());
            }
            other => panic!("Expected AssetRef, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_resolve_text() {
        let content_block_store = Arc::new(MockTextStore::new());
        let coordinator = make_coordinator(content_block_store.clone());

        // Store some text
        let block = ContentBlock::Text {
            text: "Test text".to_string(),
        };
        let stored = coordinator
            .store_content_block(block, OriginKind::User)
            .await
            .unwrap();

        // Resolve it back
        let resolved = stored.resolve(&coordinator).await.unwrap();

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
        let content_block_store = Arc::new(MockTextStore::new());
        let coordinator = make_coordinator_with_stores(
            blob_store.clone(),
            asset_store.clone(),
            content_block_store,
        );

        let original_data = b"fake image bytes";
        let image_data = STANDARD.encode(original_data);
        let block = ContentBlock::Image {
            data: image_data,
            mime_type: "image/png".to_string(),
        };

        let stored = coordinator
            .store_content_block(block, OriginKind::User)
            .await
            .unwrap();

        // Resolve it back
        let resolved = stored.resolve(&coordinator).await.unwrap();

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
        let content_block_store = Arc::new(MockTextStore::new());
        let coordinator = make_coordinator(content_block_store);

        let tool_call = llm::ToolCall {
            id: "call-1".to_string(),
            name: "test_tool".to_string(),
            arguments: serde_json::json!({"key": "value"}),
        };
        let block = ContentBlock::ToolCall(tool_call.clone());

        let result = coordinator
            .store_content_block(block, OriginKind::Assistant)
            .await
            .unwrap();

        match &result {
            StoredContent::ToolCall(stored_call) => {
                assert_eq!(stored_call.name, "test_tool");
            }
            other => panic!("Expected ToolCall, got {:?}", other),
        }
    }
}
