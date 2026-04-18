//! Document CRUD service with embedding integration.

use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;
use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::traits::{StorageTypes, Stores};
use simply_rpc::RequestContext;
use crate::api::*;

pub struct DocumentService<S: StorageTypes> {
    stores: Arc<dyn Stores<S>>,
    embedding_queue: Option<Arc<dyn crate::services::embedding_queue::EmbeddingQueue>>,
    vector_store: Option<Arc<dyn simply_core::embedding::VectorStore>>,
}

impl<S: StorageTypes> DocumentService<S> {
    pub fn new(stores: Arc<dyn Stores<S>>) -> Self {
        Self { stores, embedding_queue: None, vector_store: None }
    }

    /// Attach an embedding queue and vector store for automatic indexing.
    pub fn with_embedding(
        mut self,
        queue: Arc<dyn crate::services::embedding_queue::EmbeddingQueue>,
        vector_store: Arc<dyn simply_core::embedding::VectorStore>,
    ) -> Self {
        self.embedding_queue = Some(queue);
        self.vector_store = Some(vector_store);
        self
    }

    /// Extract user_id from RequestContext, failing if not authenticated.
    fn require_user(ctx: &RequestContext) -> anyhow::Result<simply_core::storage::ids::UserId> {
        ctx.scope.user_id.as_ref()
            .map(|id| simply_core::storage::ids::UserId::from_string(id))
            .ok_or_else(|| anyhow::anyhow!("authentication required"))
    }

    /// Enqueue a tab for embedding if a queue is configured.
    async fn enqueue_embedding(&self, tab_id: &simply_core::storage::ids::TabId, document_id: &simply_core::storage::ids::DocumentId, document_type: &str, user_id: &simply_core::storage::ids::UserId, text: &str) {
        if let Some(ref queue) = self.embedding_queue {
            if !text.is_empty() {
                queue.enqueue(crate::services::embedding_queue::EmbedJob {
                    tab_id: tab_id.clone(),
                    document_id: document_id.clone(),
                    document_type: document_type.to_string(),
                    user_id: user_id.clone(),
                    text: text.to_string(),
                }).await;
            }
        }
    }

    /// Verify the user can access a tab's parent document.
    async fn verify_tab_access(&self, user_id: &simply_core::storage::ids::UserId, tab_id: &simply_core::storage::ids::TabId, require_owner: bool) -> anyhow::Result<()> {
        use simply_core::storage::traits::DocumentStore;
        let tab = self.stores.document().get_document_tab(tab_id).await?
            .ok_or_else(|| anyhow::anyhow!("tab not found: {tab_id}"))?;
        self.verify_document_access(user_id, &tab.document_id, require_owner).await
    }

    /// Verify the user owns this document or it's public.
    async fn verify_document_access(&self, user_id: &simply_core::storage::ids::UserId, document_id: &simply_core::storage::ids::DocumentId, require_owner: bool) -> anyhow::Result<()> {
        use simply_core::storage::traits::DocumentStore;
        let doc = self.stores.document().get_document(document_id).await?
            .ok_or_else(|| anyhow::anyhow!("document not found: {document_id}"))?;
        if doc.user_id == *user_id {
            return Ok(());
        }
        if !require_owner && doc.is_public {
            return Ok(());
        }
        anyhow::bail!("access denied: document belongs to another user");
    }

    /// Delete a document, its tabs, and all associated embeddings.
    /// Does not check ownership — caller must verify access first.
    async fn delete_document_with_embeddings(&self, id: &simply_core::storage::ids::DocumentId) -> anyhow::Result<()> {
        use simply_core::storage::traits::DocumentStore;

        // Delete embeddings first (tabs are about to disappear)
        if let Some(ref vs) = self.vector_store {
            vs.delete_by_document(id).await?;
        }
        // Delete tabs explicitly (don't rely on CASCADE)
        let tabs = self.stores.document().list_document_tabs(id).await?;
        for tab in &tabs {
            self.stores.document().delete_document_tab(&tab.id).await?;
        }
        self.stores.document().delete_document(id).await?;
        Ok(())
    }

    async fn docs_to_infos(&self, docs: Vec<simply_core::storage::traits::StoredDocument>) -> anyhow::Result<Vec<DocumentInfo>> {
        use simply_core::storage::traits::DocumentStore;
        let mut result = Vec::new();
        for doc in docs {
            let tabs = self.stores.document().list_document_tabs(&doc.id).await?;
            result.push(DocumentInfo {
                id: doc.id.to_string(),
                user_id: doc.user_id.to_string(),
                title: doc.title.clone(),
                document_type: doc.document_type.clone(),
                source: format!("{:?}", doc.source),
                source_id: doc.source_id.clone(),
                tab_count: tabs.len(),
                created_at: doc.content.created_at,
                updated_at: doc.content.updated_at,
            });
        }
        Ok(result)
    }
}

#[async_trait]
impl<S: StorageTypes> DocumentApi for DocumentService<S> {
    async fn list_documents(&self, ctx: &RequestContext) -> anyhow::Result<Vec<DocumentInfo>> {
        use simply_core::storage::traits::DocumentStore;
        let user_id = Self::require_user(ctx)?;
        let docs = self.stores.document().list_documents(&user_id).await?;
        self.docs_to_infos(docs).await
    }

    async fn list_all_documents(&self) -> anyhow::Result<Vec<DocumentInfo>> {
        use simply_core::storage::traits::DocumentStore;
        let docs = self.stores.document().list_all_documents().await?;
        self.docs_to_infos(docs).await
    }

    async fn search_documents(&self, ctx: &RequestContext, query: &str) -> anyhow::Result<Vec<DocumentInfo>> {
        use simply_core::storage::traits::DocumentStore;
        let user_id = Self::require_user(ctx)?;
        let docs = self.stores.document().search_documents(&user_id, query, 50).await?;
        self.docs_to_infos(docs).await
    }

    async fn get_document(&self, ctx: &RequestContext, document_id: &str) -> anyhow::Result<DocumentDetail> {
        use simply_core::storage::ids::DocumentId;
        use simply_core::storage::traits::DocumentStore;

        let user_id = Self::require_user(ctx)?;
        let id = DocumentId::from_string(document_id);
        self.verify_document_access(&user_id, &id, false).await?;
        let doc = self.stores.document().get_document(&id).await?
            .ok_or_else(|| anyhow::anyhow!("document not found: {document_id}"))?;

        let tabs = self.stores.document().list_document_tabs(&id).await?;
        let tab_infos: Vec<TabInfo> = tabs.iter().map(|t| TabInfo {
            id: t.id.to_string(),
            title: t.title.clone(),
            icon: t.icon.clone(),
            parent_tab_id: t.parent_tab_id.as_ref().map(|id| id.to_string()),
            tab_index: t.tab_index,
            content_markdown: t.content_markdown.clone(),
            created_at: t.content.created_at,
            updated_at: t.content.updated_at,
        }).collect();

        Ok(DocumentDetail {
            id: doc.id.to_string(),
            title: doc.title.clone(),
            document_type: doc.document_type.clone(),
            source: format!("{:?}", doc.source),
            source_id: doc.source_id.clone(),
            tabs: tab_infos,
            created_at: doc.content.created_at,
            updated_at: doc.content.updated_at,
        })
    }

    async fn create_document(&self, ctx: &RequestContext, request: CreateDocumentRequest) -> anyhow::Result<DocumentInfo> {
        use simply_core::storage::traits::DocumentStore;
        use simply_core::storage::types::DocumentSource;

        let user_id = Self::require_user(ctx)?;
        let source = if request.source_id.is_some() { DocumentSource::GoogleDrive } else { DocumentSource::UserCreated };

        // If source_id is set, delete any existing document with the same source
        // (re-import replaces the old version, including its embeddings)
        if let Some(ref sid) = request.source_id {
            if let Some(existing) = self.stores.document().get_document_by_source(&user_id, source.clone(), sid).await? {
                self.delete_document_with_embeddings(&existing.id).await?;
            }
        }

        let doc_id = self.stores.document().create_document(
            &user_id,
            &request.title,
            request.document_type.as_deref().unwrap_or(simply_core::storage::types::DocumentType::DOCUMENT),
            source,
            request.source_id.as_deref(),
        ).await?;

        // Create initial tab if content provided
        if let Some(ref content) = request.content {
            let tab_id = self.stores.document().create_document_tab(
                &doc_id,
                None, // no parent
                0,    // first tab
                &request.title,
                None, // no icon
                Some(content),
                &[],  // no assets
                None, // no source tab
            ).await?;

            let doc_type = request.document_type.as_deref().unwrap_or(simply_core::storage::types::DocumentType::DOCUMENT);
            self.enqueue_embedding(&tab_id, &doc_id, doc_type, &user_id, content).await;
        }

        let doc = self.stores.document().get_document(&doc_id).await?
            .ok_or_else(|| anyhow::anyhow!("document not found after create"))?;

        self.docs_to_infos(vec![doc]).await.map(|mut v| v.remove(0))
    }

    async fn rename_document(&self, ctx: &RequestContext, document_id: &str, title: &str) -> anyhow::Result<()> {
        use simply_core::storage::ids::DocumentId;
        use simply_core::storage::traits::DocumentStore;
        let user_id = Self::require_user(ctx)?;
        let id = DocumentId::from_string(document_id);
        self.verify_document_access(&user_id, &id, true).await?;
        self.stores.document().update_document_title(&id, title).await
    }

    async fn delete_document(&self, ctx: &RequestContext, document_id: &str) -> anyhow::Result<()> {
        use simply_core::storage::ids::DocumentId;
        let user_id = Self::require_user(ctx)?;
        let id = DocumentId::from_string(document_id);
        self.verify_document_access(&user_id, &id, true).await?;
        self.delete_document_with_embeddings(&id).await
    }

    async fn create_tab(&self, ctx: &RequestContext, document_id: &str, request: CreateTabRequest) -> anyhow::Result<TabInfo> {
        use simply_core::storage::ids::{DocumentId, TabId};
        use simply_core::storage::traits::DocumentStore;

        let user_id = Self::require_user(ctx)?;
        let doc_id = DocumentId::from_string(document_id);
        self.verify_document_access(&user_id, &doc_id, true).await?;
        let parent = request.parent_tab_id.as_deref().map(TabId::from_string);

        let tab_id = self.stores.document().create_document_tab(
            &doc_id,
            parent.as_ref(),
            request.tab_index.unwrap_or(0),
            &request.title,
            None,
            request.content.as_deref(),
            &[],
            None,
        ).await?;

        // Enqueue for embedding
        if let Some(ref content) = request.content {
            let doc = self.stores.document().get_document(&doc_id).await?;
            let doc_type = doc.as_ref().map(|d| d.document_type.as_str()).unwrap_or("document");
            self.enqueue_embedding(&tab_id, &doc_id, doc_type, &user_id, content).await;
        }

        let tab = self.stores.document().get_document_tab(&tab_id).await?
            .ok_or_else(|| anyhow::anyhow!("tab not found after create"))?;

        Ok(TabInfo {
            id: tab.id.to_string(),
            title: tab.title.clone(),
            icon: tab.icon.clone(),
            parent_tab_id: tab.parent_tab_id.as_ref().map(|id| id.to_string()),
            tab_index: tab.tab_index,
            content_markdown: tab.content_markdown.clone(),
            created_at: tab.content.created_at,
            updated_at: tab.content.updated_at,
        })
    }

    async fn get_tab(&self, ctx: &RequestContext, tab_id: &str) -> anyhow::Result<TabInfo> {
        use simply_core::storage::ids::TabId;
        use simply_core::storage::traits::DocumentStore;

        let user_id = Self::require_user(ctx)?;
        let tid = TabId::from_string(tab_id);
        self.verify_tab_access(&user_id, &tid, false).await?;
        let tab = self.stores.document().get_document_tab(&tid).await?
            .ok_or_else(|| anyhow::anyhow!("tab not found: {tab_id}"))?;

        Ok(TabInfo {
            id: tab.id.to_string(),
            title: tab.title.clone(),
            icon: tab.icon.clone(),
            parent_tab_id: tab.parent_tab_id.as_ref().map(|id| id.to_string()),
            tab_index: tab.tab_index,
            content_markdown: tab.content_markdown.clone(),
            created_at: tab.content.created_at,
            updated_at: tab.content.updated_at,
        })
    }

    async fn update_tab(&self, ctx: &RequestContext, tab_id: &str, request: UpdateTabRequest) -> anyhow::Result<()> {
        use simply_core::storage::ids::TabId;
        use simply_core::storage::traits::DocumentStore;
        let user_id = Self::require_user(ctx)?;
        let tid = TabId::from_string(tab_id);
        self.verify_tab_access(&user_id, &tid, true).await?;
        self.stores.document().update_document_tab_content(&tid, &request.content, &[]).await?;

        // Re-embed updated content
        let tab = self.stores.document().get_document_tab(&tid).await?;
        if let Some(tab) = tab {
            let doc = self.stores.document().get_document(&tab.document_id).await?;
            let doc_type = doc.as_ref().map(|d| d.document_type.as_str()).unwrap_or("document");
            self.enqueue_embedding(&tid, &tab.document_id, doc_type, &user_id, &request.content).await;
        }

        Ok(())
    }

    async fn delete_tab(&self, ctx: &RequestContext, tab_id: &str) -> anyhow::Result<()> {
        use simply_core::storage::ids::TabId;
        use simply_core::storage::traits::DocumentStore;
        let user_id = Self::require_user(ctx)?;
        let tid = TabId::from_string(tab_id);
        self.verify_tab_access(&user_id, &tid, true).await?;
        self.stores.document().delete_document_tab(&tid).await?;
        Ok(())
    }

    async fn flush_tab_embedding(&self, _ctx: &RequestContext, tab_id: &str) -> anyhow::Result<()> {
        if let Some(ref queue) = self.embedding_queue {
            queue.flush(tab_id).await;
        }
        Ok(())
    }
}

