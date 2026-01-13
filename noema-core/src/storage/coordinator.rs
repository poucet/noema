//! Storage coordinator - orchestrates all storage operations
//!
//! The coordinator provides a unified interface for:
//! - Converting LLM content to stored references (text, assets, documents)
//! - Conversation lifecycle and structure (via ConversationStore)
//! - User management (via UserStore)
//! - Document management (via DocumentStore)
//!
//! It also implements `ContentResolver` and `DocumentResolver` for resolving refs.

use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use llm::ContentBlock;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::storage::content::{ContentResolver, InputContent, StoredContent};
use crate::storage::ids::{AssetId, ContentBlockId, ConversationId, SpanId, UserId, ViewId};
use crate::storage::session::{ResolvedContent, ResolvedMessage};
use crate::storage::traits::{
    AssetStore, BlobStore, ConversationStore, StorageTypes, TextStore, TurnStore, UserStore,
};
use crate::storage::types::{
    Asset, ContentBlock as ContentBlockData, ContentOrigin, ConversationInfo, MessageRole,
    OriginKind, TurnWithContent, UserInfo,
};

/// Coordinates storage across all store types.
///
/// Generic over `S: StorageTypes` which bundles all storage type associations.
/// Access individual stores via associated types: `S::Blob`, `S::Asset`, etc.
pub struct StorageCoordinator<S: StorageTypes> {
    blob_store: Arc<S::Blob>,
    asset_store: Arc<S::Asset>,
    content_block_store: Arc<S::Text>,
    conversation_store: Arc<S::Conversation>,
    turn_store: Arc<S::Turn>,
    user_store: Arc<S::User>,
    document_store: Arc<S::Document>,
    _marker: PhantomData<S>,
}

impl<S: StorageTypes> StorageCoordinator<S> {
    /// Create a new storage coordinator
    pub fn new(
        blob_store: Arc<S::Blob>,
        asset_store: Arc<S::Asset>,
        content_block_store: Arc<S::Text>,
        conversation_store: Arc<S::Conversation>,
        turn_store: Arc<S::Turn>,
        user_store: Arc<S::User>,
        document_store: Arc<S::Document>,
    ) -> Self {
        Self {
            blob_store,
            asset_store,
            content_block_store,
            conversation_store,
            turn_store,
            user_store,
            document_store,
            _marker: PhantomData,
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
                    let asset_id = self.store_base64_asset(&data, &mime_type).await?;
                    StoredContent::asset_ref(asset_id, mime_type, None)
                }
                InputContent::Audio { data, mime_type } => {
                    let asset_id = self.store_base64_asset(&data, &mime_type).await?;
                    StoredContent::asset_ref(asset_id, mime_type, None)
                }
                InputContent::DocumentRef { id } => StoredContent::document_ref(id),
                InputContent::AssetRef { asset_id, mime_type } => {
                    StoredContent::asset_ref(asset_id, mime_type, None)
                }
            };
            stored.push(stored_item);
        }

        Ok(stored)
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

    /// Get blob data by hash
    pub async fn get_blob(&self, hash: &str) -> Result<Vec<u8>> {
        self.blob_store.get(hash).await
    }

    /// Get access to the turn store
    pub fn turn_store(&self) -> &Arc<S::Turn> {
        &self.turn_store
    }


    /// Get access to the document store
    pub fn document_store(&self) -> &Arc<S::Document> {
        &self.document_store
    }

    // ========== Conversation Delegation Methods ==========

    /// Create a new conversation for a user
    pub async fn create_conversation(
        &self,
        user_id: &UserId,
        name: Option<&str>,
    ) -> Result<ConversationId> {
        self.conversation_store.create_conversation(user_id, name).await
    }

    /// List all conversations for a user
    pub async fn list_conversations(&self, user_id: &UserId) -> Result<Vec<ConversationInfo>> {
        self.conversation_store.list_conversations(user_id).await
    }

    /// Delete a conversation and all its data
    pub async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()> {
        self.conversation_store.delete_conversation(conversation_id).await
    }

    /// Rename a conversation
    pub async fn rename_conversation(
        &self,
        conversation_id: &ConversationId,
        name: Option<&str>,
    ) -> Result<()> {
        self.conversation_store.rename_conversation(conversation_id, name).await
    }

    /// Get privacy setting for a conversation
    pub async fn is_conversation_private(&self, conversation_id: &ConversationId) -> Result<bool> {
        self.conversation_store.is_conversation_private(conversation_id).await
    }

    /// Set privacy setting for a conversation
    pub async fn set_conversation_private(
        &self,
        conversation_id: &ConversationId,
        is_private: bool,
    ) -> Result<()> {
        self.conversation_store.set_conversation_private(conversation_id, is_private).await
    }

    /// Get the main view for a conversation
    ///
    /// Looks up the conversation and returns its main view info.
    pub async fn get_main_view(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<crate::storage::types::ViewInfo> {
        let conv = self.conversation_store
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Conversation not found: {}", conversation_id))?;

        self.turn_store
            .get_view(&conv.main_view_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Main view not found: {}", conv.main_view_id))
    }

    // ========== Turn/Span Methods ==========

    /// Start a new turn in a conversation view.
    ///
    /// Creates a turn, adds a span to it, and selects that span in the view.
    /// Returns the span ID for adding messages.
    pub async fn start_turn(
        &self,
        view_id: &ViewId,
        role: crate::storage::types::SpanRole,
        model_id: Option<&str>,
    ) -> Result<SpanId> {
        let turn = self.turn_store.create_turn(role).await?;
        let span = self.turn_store.create_span(&turn.id, model_id).await?;
        self.turn_store.select_span(view_id, &turn.id, &span.id).await?;
        Ok(span.id)
    }

    // ========== User Delegation Methods ==========

    /// Get or create the default user
    pub async fn get_or_create_default_user(&self) -> Result<UserInfo> {
        self.user_store.get_or_create_default_user().await
    }

    /// Get or create a user by email
    pub async fn get_or_create_user_by_email(&self, email: &str) -> Result<UserInfo> {
        self.user_store.get_or_create_user_by_email(email).await
    }

    /// List all users
    pub async fn list_users(&self) -> Result<Vec<UserInfo>> {
        self.user_store.list_users().await
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
            for msg in &turn.messages {
                let resolved = self.resolve_stored_content(&msg.content).await?;
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
                    ResolvedContent::Asset {
                        asset_id: asset_id.clone(),
                        blob_hash,
                        mime_type: mime_type.clone(),
                        filename: filename.clone(),
                        resolved: resolved_block,
                    }
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
        role: MessageRole,
        content: Vec<ContentBlock>,
        origin: OriginKind,
    ) -> Result<ResolvedMessage> {
        // Store content blocks
        let stored = self.store_content(content, origin).await?;

        // Add message to turn store
        self.turn_store.add_message(span_id, role, &stored).await?;

        // Resolve for caching
        let resolved = self.resolve_stored_content(&stored).await?;

        Ok(ResolvedMessage::new(role, resolved))
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

/// Implement DocumentResolver by delegating to the document store
#[async_trait]
impl<S: StorageTypes> crate::storage::DocumentResolver for StorageCoordinator<S> {
    async fn resolve_documents(
        &self,
        doc_ids: &[crate::storage::ids::DocumentId],
    ) -> std::collections::HashMap<crate::storage::ids::DocumentId, crate::storage::types::FullDocumentInfo> {
        self.document_store.resolve_documents(doc_ids).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::content::StoredContent;
    use crate::storage::ids::{ConversationId, DocumentId, MessageId, RevisionId, SpanId, TabId, TurnId, ViewId};
    use crate::storage::traits::{AssetStore, BlobStore, ConversationStore, DocumentStore, TextStore, TurnStore, UserStore};
    use crate::storage::types::{
        DocumentInfo, DocumentRevisionInfo, DocumentSource, DocumentTabInfo, FullDocumentInfo,
        MessageInfo, MessageRole, MessageWithContent, SpanInfo, SpanRole, StoredAsset,
        StoredBlob, StoreResult, StoredContentBlock, TurnInfo, TurnWithContent, ViewInfo,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    /// Mock storage types bundled together
    struct MockStorage;

    impl StorageTypes for MockStorage {
        type Blob = MockBlobStore;
        type Asset = MockAssetStore;
        type Text = MockTextStore;
        type Conversation = MockConversationStore;
        type Turn = MockTurnStore;
        type User = MockUserStore;
        type Document = MockDocumentStore;
    }

    // Mock conversation store
    struct MockConversationStore;

    #[async_trait]
    impl ConversationStore for MockConversationStore {
        async fn create_conversation(&self, _: &UserId, _: Option<&str>) -> Result<ConversationId> { unimplemented!() }
        async fn get_conversation(&self, _: &ConversationId) -> Result<Option<ConversationInfo>> { unimplemented!() }
        async fn list_conversations(&self, _: &UserId) -> Result<Vec<ConversationInfo>> { unimplemented!() }
        async fn delete_conversation(&self, _: &ConversationId) -> Result<()> { unimplemented!() }
        async fn rename_conversation(&self, _: &ConversationId, _: Option<&str>) -> Result<()> { unimplemented!() }
        async fn is_conversation_private(&self, _: &ConversationId) -> Result<bool> { unimplemented!() }
        async fn set_conversation_private(&self, _: &ConversationId, _: bool) -> Result<()> { unimplemented!() }
        async fn set_main_view_id(&self, _: &ConversationId, _: &ViewId) -> Result<()> { unimplemented!() }
    }

    // Mock turn store
    struct MockTurnStore;

    #[async_trait]
    impl TurnStore for MockTurnStore {
        async fn create_turn(&self, _: SpanRole) -> Result<TurnInfo> { unimplemented!() }
        async fn get_turn(&self, _: &TurnId) -> Result<Option<TurnInfo>> { unimplemented!() }
        async fn create_span(&self, _: &TurnId, _: Option<&str>) -> Result<SpanInfo> { unimplemented!() }
        async fn get_spans(&self, _: &TurnId) -> Result<Vec<SpanInfo>> { unimplemented!() }
        async fn get_span(&self, _: &SpanId) -> Result<Option<SpanInfo>> { unimplemented!() }
        async fn add_message(&self, _: &SpanId, _: MessageRole, _: &[StoredContent]) -> Result<MessageInfo> { unimplemented!() }
        async fn get_messages(&self, _: &SpanId) -> Result<Vec<MessageWithContent>> { unimplemented!() }
        async fn get_message(&self, _: &MessageId) -> Result<Option<MessageInfo>> { unimplemented!() }
        async fn create_view(&self) -> Result<ViewInfo> { unimplemented!() }
        async fn get_view(&self, _: &ViewId) -> Result<Option<ViewInfo>> { unimplemented!() }
        async fn select_span(&self, _: &ViewId, _: &TurnId, _: &SpanId) -> Result<()> { unimplemented!() }
        async fn get_selected_span(&self, _: &ViewId, _: &TurnId) -> Result<Option<SpanId>> { unimplemented!() }
        async fn get_view_path(&self, _: &ViewId) -> Result<Vec<TurnWithContent>> { unimplemented!() }
        async fn fork_view(&self, _: &ViewId, _: &TurnId) -> Result<ViewInfo> { unimplemented!() }
        async fn get_view_context_at(&self, _: &ViewId, _: &TurnId) -> Result<Vec<TurnWithContent>> { unimplemented!() }
        async fn edit_turn(&self, _: &ViewId, _: &TurnId, _: Vec<(MessageRole, Vec<StoredContent>)>, _: Option<&str>, _: bool) -> Result<(SpanInfo, Option<ViewInfo>)> { unimplemented!() }
    }

    // Mock user store
    struct MockUserStore;

    #[async_trait]
    impl UserStore for MockUserStore {
        async fn get_or_create_default_user(&self) -> Result<UserInfo> { unimplemented!() }
        async fn get_user_by_email(&self, _: &str) -> Result<Option<UserInfo>> { unimplemented!() }
        async fn get_or_create_user_by_email(&self, _: &str) -> Result<UserInfo> { unimplemented!() }
        async fn list_users(&self) -> Result<Vec<UserInfo>> { unimplemented!() }
    }

    // Mock document store
    struct MockDocumentStore;

    #[async_trait]
    impl DocumentStore for MockDocumentStore {
        async fn create_document(&self, _: &UserId, _: &str, _: DocumentSource, _: Option<&str>) -> Result<DocumentId> { unimplemented!() }
        async fn get_document(&self, _: &DocumentId) -> Result<Option<DocumentInfo>> { unimplemented!() }
        async fn get_document_by_source(&self, _: &UserId, _: DocumentSource, _: &str) -> Result<Option<DocumentInfo>> { unimplemented!() }
        async fn list_documents(&self, _: &UserId) -> Result<Vec<DocumentInfo>> { unimplemented!() }
        async fn search_documents(&self, _: &UserId, _: &str, _: usize) -> Result<Vec<DocumentInfo>> { unimplemented!() }
        async fn update_document_title(&self, _: &DocumentId, _: &str) -> Result<()> { unimplemented!() }
        async fn delete_document(&self, _: &DocumentId) -> Result<bool> { unimplemented!() }
        async fn create_document_tab(&self, _: &DocumentId, _: Option<&TabId>, _: i32, _: &str, _: Option<&str>, _: Option<&str>, _: &[AssetId], _: Option<&TabId>) -> Result<TabId> { unimplemented!() }
        async fn get_document_tab(&self, _: &TabId) -> Result<Option<DocumentTabInfo>> { unimplemented!() }
        async fn list_document_tabs(&self, _: &DocumentId) -> Result<Vec<DocumentTabInfo>> { unimplemented!() }
        async fn update_document_tab_content(&self, _: &TabId, _: &str, _: &[AssetId]) -> Result<()> { unimplemented!() }
        async fn update_document_tab_parent(&self, _: &TabId, _: Option<&TabId>) -> Result<()> { unimplemented!() }
        async fn set_document_tab_revision(&self, _: &TabId, _: &RevisionId) -> Result<()> { unimplemented!() }
        async fn delete_document_tab(&self, _: &TabId) -> Result<bool> { unimplemented!() }
        async fn create_document_revision(&self, _: &TabId, _: &str, _: &str, _: &[AssetId], _: &UserId) -> Result<RevisionId> { unimplemented!() }
        async fn get_document_revision(&self, _: &RevisionId) -> Result<Option<DocumentRevisionInfo>> { unimplemented!() }
        async fn list_document_revisions(&self, _: &TabId) -> Result<Vec<DocumentRevisionInfo>> { unimplemented!() }
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
    struct MockTextStore {
        blocks: Mutex<HashMap<String, String>>,
        counter: Mutex<u64>,
    }

    impl MockTextStore {
        fn new() -> Self {
            Self {
                blocks: Mutex::new(HashMap::new()),
                counter: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl TextStore for MockTextStore {
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

    fn make_coordinator(content_block_store: Arc<MockTextStore>) -> StorageCoordinator<MockStorage> {
        StorageCoordinator::new(
            Arc::new(MockBlobStore::new()),
            Arc::new(MockAssetStore::new()),
            content_block_store,
            Arc::new(MockConversationStore),
            Arc::new(MockTurnStore),
            Arc::new(MockUserStore),
            Arc::new(MockDocumentStore),
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
            Arc::new(MockUserStore),
            Arc::new(MockDocumentStore),
        )
    }

    #[tokio::test]
    async fn test_store_text_content() {
        let content_block_store = Arc::new(MockTextStore::new());
        let coordinator = make_coordinator(content_block_store.clone());

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
        let content_block_store = Arc::new(MockTextStore::new());
        let coordinator = make_coordinator_with_stores(
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
        let content_block_store = Arc::new(MockTextStore::new());
        let coordinator = make_coordinator_with_stores(
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
        let content_block_store = Arc::new(MockTextStore::new());
        let coordinator = make_coordinator(content_block_store);

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
