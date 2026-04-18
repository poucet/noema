//! Semantic search service.

use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;
use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::traits::{StorageTypes, Stores};
use simply_rpc::RequestContext;
use crate::api::*;

pub struct SearchService<S: StorageTypes> {
    embedding_provider: Arc<dyn llm::EmbeddingProvider>,
    vector_store: Arc<dyn simply_core::embedding::VectorStore>,
    embedding_queue: Arc<dyn crate::services::embedding_queue::EmbeddingQueue>,
    stores: Arc<dyn Stores<S>>,
}

impl<S: StorageTypes> SearchService<S> {
    pub fn new(
        embedding_provider: Arc<dyn llm::EmbeddingProvider>,
        vector_store: Arc<dyn simply_core::embedding::VectorStore>,
        embedding_queue: Arc<dyn crate::services::embedding_queue::EmbeddingQueue>,
        stores: Arc<dyn Stores<S>>,
    ) -> Self {
        Self { embedding_provider, vector_store, embedding_queue, stores }
    }
}

#[async_trait]
impl<S: StorageTypes> SearchApi for SearchService<S> {
    async fn search(&self, ctx: &RequestContext, request: SearchRequest) -> anyhow::Result<Vec<SearchHit>> {
        let top_k = request.top_k.unwrap_or(5);
        let user_id = ctx.scope.user_id.as_ref()
            .map(|id| simply_core::storage::ids::UserId::from_string(id));

        // Embed the query
        let embeddings = self.embedding_provider.embed(&[&request.query]).await?;
        let vector = embeddings.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("embedding provider returned no vectors"))?
            .vector;

        // Search
        let filter = simply_core::embedding::SearchFilter {
            document_type: request.document_type,
            user_id,
        };

        let results = self.vector_store.search(simply_core::embedding::SearchQuery {
            vector,
            top_k,
            filter: Some(filter),
        }).await?;

        // Enrich with document titles
        let mut hits = Vec::with_capacity(results.len());
        for result in results {
            use simply_core::storage::traits::DocumentStore;
            let title = self.stores.document()
                .get_document(&result.chunk.document_id).await?
                .map(|d| d.title.clone())
                .unwrap_or_else(|| "Untitled".to_string());

            hits.push(SearchHit {
                document_id: result.chunk.document_id.to_string(),
                document_title: title,
                document_type: result.chunk.document_type,
                tab_id: result.chunk.tab_id.to_string(),
                chunk_text: result.chunk.text,
                chunk_index: result.chunk.chunk_index,
                score: result.score,
            });
        }

        Ok(hits)
    }

    async fn reindex(&self, _ctx: &RequestContext) -> anyhow::Result<ReindexStatus> {
        use simply_core::storage::traits::DocumentStore;

        // Delete all existing vectors
        self.vector_store.delete_all().await?;

        // Scan all documents across all users and enqueue all tabs
        let docs = self.stores.document().list_all_documents().await?;
        let mut tabs_queued = 0;

        for doc in &docs {
            let tabs = self.stores.document().list_document_tabs(&doc.id).await?;
            for tab in tabs {
                if let Some(ref content) = tab.content_markdown {
                    if !content.is_empty() {
                        self.embedding_queue.enqueue(crate::services::embedding_queue::EmbedJob {
                            tab_id: tab.id.clone(),
                            document_id: doc.id.clone(),
                            document_type: doc.document_type.clone(),
                            user_id: doc.user_id.clone(),
                            text: content.clone(),
                        }).await;
                        tabs_queued += 1;
                    }
                }
            }
        }

        Ok(ReindexStatus {
            message: format!("Reindex started: {} tabs queued", tabs_queued),
            tabs_queued,
        })
    }

    async fn queue_status(&self, _ctx: &RequestContext) -> anyhow::Result<crate::services::embedding_queue::EmbeddingQueueStatus> {
        Ok(self.embedding_queue.status().await)
    }
}
