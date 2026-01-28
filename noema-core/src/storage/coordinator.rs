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
use llm::{ContentBlock, Role};
use std::marker::PhantomData;
use std::sync::Arc;

use crate::storage::content::{ContentResolver, InputContent, StoredContent};
use crate::storage::ids::{AssetId, ContentBlockId, ConversationId, SpanId, TurnId, UserId, ViewId};
use crate::storage::session::{ResolvedContent, ResolvedMessage};
use crate::storage::traits::{
    AssetStore, BlobStore, EntityStore, StorageTypes, Stores, TextStore, TurnStore,
};
use crate::storage::types::{
    Asset, BlobHash, ContentBlock as ContentBlockData, ContentOrigin, EntityType, OriginKind,
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
    entity_store: Arc<S::Entity>,
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
            entity_store: stores.entity(),
            turn_store: stores.turn(),
            _marker: PhantomData,
        }
    }

    /// Create a new storage coordinator from individual store instances
    pub fn new(
        blob_store: Arc<S::Blob>,
        asset_store: Arc<S::Asset>,
        content_block_store: Arc<S::Text>,
        entity_store: Arc<S::Entity>,
        turn_store: Arc<S::Turn>,
    ) -> Self {
        Self {
            blob_store,
            asset_store,
            content_block_store,
            entity_store,
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
                let content_block =
                    ContentBlockData::plain(&text).with_origin(ContentOrigin::from_kind(origin));
                let content_block_id = self.content_block_store.store(content_block).await?;
                Ok(StoredContent::text_ref(content_block_id))
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
                    let content_block =
                        ContentBlockData::plain(&text).with_origin(ContentOrigin::from_kind(origin));
                    let content_block_id = self.content_block_store.store(content_block).await?;
                    StoredContent::text_ref(content_block_id)
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
        let blob_hash = self.blob_store.store(&bytes).await?;
        let asset = Asset::new(blob_hash, mime_type, bytes.len() as i64);
        self.asset_store.create_asset(asset).await
    }

    /// Get blob data by hash
    pub async fn get_blob(&self, hash: &BlobHash) -> Result<Vec<u8>> {
        self.blob_store.get(hash).await
    }

    // ========== Turn/Span Methods ==========

    /// Create a new turn (without span or selection).
    pub async fn create_turn(&self, role: llm::Role) -> Result<TurnId> {
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
    /// 1. Getting the conversation entity and extracting main_view_id from metadata
    /// 2. Loading the view path (turns with content)
    /// 3. Resolving stored content to resolved messages
    ///
    /// Returns (view_id, resolved_messages) for Session construction.
    pub async fn open_session(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<(ViewId, Vec<ResolvedMessage>)> {
        // Get conversation entity to extract main_view_id from metadata
        let entity = self.entity_store
            .get_entity(conversation_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Conversation not found: {}", conversation_id))?;

        // Extract main_view_id from metadata
        let main_view_id = entity.metadata
            .as_ref()
            .and_then(|m| m.get("main_view_id"))
            .and_then(|v| v.as_str())
            .map(ViewId::from_string)
            .ok_or_else(|| anyhow::anyhow!("Conversation has no main_view_id: {}", conversation_id))?;

        // Open session with the main view
        self.open_session_with_view(&main_view_id).await
            .map(|resolved| (main_view_id, resolved))
    }

    /// Create a new conversation with its main view.
    ///
    /// This method handles the multi-store coordination of:
    /// 1. Creating the conversation entity
    /// 2. Creating the main view
    /// 3. Setting the main_view_id in entity metadata
    ///
    /// Returns the ConversationId (EntityId) for further operations.
    pub async fn create_conversation_with_view(
        &self,
        user_id: &UserId,
        name: Option<&str>,
    ) -> Result<ConversationId> {
        // Create conversation entity
        let conversation_id = self.entity_store
            .create_entity(EntityType::conversation(), Some(user_id))
            .await?;

        // Create main view
        let view = self.turn_store.create_view().await?;

        // Get entity and update with main_view_id and name
        let mut entity = self.entity_store
            .get_entity(&conversation_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Just-created entity not found"))?;

        entity.name = name.map(|n| n.to_string());
        entity.metadata = Some(serde_json::json!({
            "main_view_id": view.id.as_str()
        }));

        self.entity_store.update_entity(&conversation_id, &entity).await?;

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

    /// Spawn a subconversation linked to a parent conversation.
    ///
    /// Creates a new conversation entity and links it to the parent via
    /// entity_relations with RelationType::spawned_from(). The metadata
    /// records which turn in the parent triggered the spawn.
    ///
    /// # Arguments
    /// * `parent_conversation_id` - The parent conversation entity ID
    /// * `user_id` - The user who owns the subconversation
    /// * `at_turn_id` - The turn in the parent where the spawn was triggered
    /// * `at_span_id` - Optional: the specific span at that turn
    /// * `name` - Optional name for the subconversation
    ///
    /// # Returns
    /// The new subconversation's ConversationId
    pub async fn spawn_subconversation(
        &self,
        parent_conversation_id: &ConversationId,
        user_id: &UserId,
        at_turn_id: &TurnId,
        at_span_id: Option<&SpanId>,
        name: Option<&str>,
    ) -> Result<ConversationId> {
        use crate::storage::types::RelationType;

        // Create the subconversation with its own view
        let sub_conversation_id = self.create_conversation_with_view(user_id, name).await?;

        // Build spawn metadata
        let mut metadata = serde_json::json!({
            "at_turn_id": at_turn_id.as_str()
        });
        if let Some(span_id) = at_span_id {
            metadata["at_span_id"] = serde_json::Value::String(span_id.as_str().to_string());
        }

        // Link subconversation → parent with spawned_from relation
        self.entity_store
            .add_relation(
                &sub_conversation_id,
                parent_conversation_id,
                RelationType::spawned_from(),
                Some(metadata),
            )
            .await?;

        Ok(sub_conversation_id)
    }

    /// Get the parent conversation for a subconversation.
    ///
    /// Returns None if the conversation has no parent (is not a subconversation).
    pub async fn get_parent_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<(ConversationId, TurnId, Option<SpanId>)>> {
        use crate::storage::types::RelationType;

        let relations = self.entity_store
            .get_relations_from(conversation_id, Some(&RelationType::spawned_from()))
            .await?;

        if let Some((parent_id, relation)) = relations.into_iter().next() {
            let at_turn_id = relation.metadata
                .as_ref()
                .and_then(|m| m.get("at_turn_id"))
                .and_then(|v| v.as_str())
                .map(TurnId::from_string)
                .ok_or_else(|| anyhow::anyhow!("spawned_from relation missing at_turn_id"))?;

            let at_span_id = relation.metadata
                .as_ref()
                .and_then(|m| m.get("at_span_id"))
                .and_then(|v| v.as_str())
                .map(SpanId::from_string);

            Ok(Some((parent_id, at_turn_id, at_span_id)))
        } else {
            Ok(None)
        }
    }

    /// List all subconversations spawned from a parent conversation.
    pub async fn list_subconversations(
        &self,
        parent_conversation_id: &ConversationId,
    ) -> Result<Vec<(ConversationId, TurnId, Option<SpanId>)>> {
        use crate::storage::types::RelationType;

        let relations = self.entity_store
            .get_relations_to(parent_conversation_id, Some(&RelationType::spawned_from()))
            .await?;

        let mut result = Vec::new();
        for (sub_id, relation) in relations {
            let at_turn_id = relation.metadata
                .as_ref()
                .and_then(|m| m.get("at_turn_id"))
                .and_then(|v| v.as_str())
                .map(TurnId::from_string);

            let at_span_id = relation.metadata
                .as_ref()
                .and_then(|m| m.get("at_span_id"))
                .and_then(|v| v.as_str())
                .map(SpanId::from_string);

            if let Some(turn_id) = at_turn_id {
                result.push((sub_id, turn_id, at_span_id));
            }
        }

        Ok(result)
    }

    /// Get the final result text from a subconversation.
    ///
    /// Returns the text content of the last assistant message in the subconversation's
    /// main view. Returns None if there are no messages or no text content.
    pub async fn get_subconversation_result(
        &self,
        subconversation_id: &ConversationId,
    ) -> Result<Option<String>> {
        // Get the main view ID from entity metadata
        let entity = self.entity_store
            .get_entity(subconversation_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Subconversation not found: {}", subconversation_id))?;

        let main_view_id = entity.metadata
            .as_ref()
            .and_then(|m| m.get("main_view_id"))
            .and_then(|v| v.as_str())
            .map(ViewId::from_string)
            .ok_or_else(|| anyhow::anyhow!("Subconversation has no main_view_id"))?;

        // Get the view path
        let path = self.turn_store.get_view_path(&main_view_id).await?;

        // Find the last assistant message
        for turn in path.into_iter().rev() {
            if turn.turn.role() == Role::Assistant {
                // Get the last message content from this turn
                for msg in turn.messages.into_iter().rev() {
                    if msg.message.role == Role::Assistant {
                        // Resolve and extract text from content
                        let resolved = self.resolve_stored_content(&msg.content).await?;
                        let text: Vec<String> = resolved
                            .into_iter()
                            .filter_map(|c| c.as_text().map(|t| t.to_string()))
                            .collect();

                        if !text.is_empty() {
                            return Ok(Some(text.join("\n")));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Link a subconversation's result back to the parent conversation.
    ///
    /// Creates a ToolResult message in the parent conversation's current span
    /// containing the subconversation's final result. This is used when a
    /// spawned agent completes and its result should appear in the parent flow.
    ///
    /// # Arguments
    /// * `subconversation_id` - The subconversation whose result to link
    /// * `parent_span_id` - The span in the parent to add the ToolResult to
    /// * `parent_turn_id` - The turn in the parent (for ResolvedMessage)
    /// * `tool_call_id` - The ID of the original ToolCall that spawned this
    /// * `tool_name` - The name of the tool (e.g., "spawn_agent")
    ///
    /// # Returns
    /// The resolved message that was added (for caching in Session)
    pub async fn link_subconversation_result(
        &self,
        subconversation_id: &ConversationId,
        parent_span_id: &SpanId,
        parent_turn_id: &TurnId,
        tool_call_id: &str,
        tool_name: &str,
    ) -> Result<ResolvedMessage> {
        // Get the subconversation's result
        let result_text = self
            .get_subconversation_result(subconversation_id)
            .await?
            .unwrap_or_else(|| "(no result)".to_string());

        // Create a ToolResult that includes both the result and a reference to the subconversation
        let result_content = format!(
            "{}\n\n[subconversation_id: {}]",
            result_text,
            subconversation_id.as_str()
        );
        let tool_result = llm::ToolResult {
            tool_call_id: tool_call_id.to_string(),
            content: vec![llm::ToolResultContent::text(result_content)],
        };

        // Add as a message in the parent span (tool results are sent as User role)
        let content = vec![ContentBlock::ToolResult(tool_result.clone())];
        self.add_message(
            parent_span_id,
            parent_turn_id,
            Role::User,
            content,
            OriginKind::System,
        )
        .await
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
                    let blob_hash = &stored_asset.blob_hash;

                    // Load blob data and create resolved ContentBlock for LLM
                    let resolved_block = match self.blob_store.get(blob_hash).await {
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
                        blob_hash.clone(),
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
        role: Role,
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
        let data = self.blob_store.get(&stored_asset.blob_hash).await?;

        Ok((data, stored_asset.mime_type.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::content::StoredContent;
    use crate::storage::implementations::mock::{
        MockAssetStore, MockBlobStore, MockEntityStore, MockStorage, MockTextStore,
        MockTurnStore,
    };
    use crate::storage::traits::AssetStore;

    fn make_coordinator(content_block_store: Arc<MockTextStore>) -> StorageCoordinator<MockStorage> {
        StorageCoordinator::new(
            Arc::new(MockBlobStore::new()),
            Arc::new(MockAssetStore::new()),
            content_block_store,
            Arc::new(MockEntityStore),
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
            Arc::new(MockEntityStore),
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
