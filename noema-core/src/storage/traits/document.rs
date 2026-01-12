//! DocumentStore trait for document, tab, and revision storage

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::types::document::{
    DocumentInfo, DocumentRevisionInfo, DocumentSource, DocumentTabInfo, FullDocumentInfo,
};

/// Trait for document storage operations
#[async_trait]
pub trait DocumentStore: Send + Sync {
    // ========== Document Methods ==========

    /// Create a new document
    async fn create_document(
        &self,
        user_id: &str,
        title: &str,
        source: DocumentSource,
        source_id: Option<&str>,
    ) -> Result<String>;

    /// Get a document by ID
    async fn get_document(&self, id: &str) -> Result<Option<DocumentInfo>>;

    /// Get a document by source and source_id (e.g., find by Google Doc ID)
    async fn get_document_by_source(
        &self,
        user_id: &str,
        source: DocumentSource,
        source_id: &str,
    ) -> Result<Option<DocumentInfo>>;

    /// List all documents for a user
    async fn list_documents(&self, user_id: &str) -> Result<Vec<DocumentInfo>>;

    /// Search documents by title (case-insensitive)
    async fn search_documents(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<DocumentInfo>>;

    /// Update document title
    async fn update_document_title(&self, id: &str, title: &str) -> Result<()>;

    /// Delete a document and all its tabs/revisions
    async fn delete_document(&self, id: &str) -> Result<bool>;

    // ========== Document Tab Methods ==========

    /// Create a new document tab
    async fn create_document_tab(
        &self,
        document_id: &str,
        parent_tab_id: Option<&str>,
        tab_index: i32,
        title: &str,
        icon: Option<&str>,
        content_markdown: Option<&str>,
        referenced_assets: &[String],
        source_tab_id: Option<&str>,
    ) -> Result<String>;

    /// Get a document tab by ID
    async fn get_document_tab(&self, id: &str) -> Result<Option<DocumentTabInfo>>;

    /// List all tabs for a document
    async fn list_document_tabs(&self, document_id: &str) -> Result<Vec<DocumentTabInfo>>;

    /// Update tab content
    async fn update_document_tab_content(
        &self,
        id: &str,
        content_markdown: &str,
        referenced_assets: &[String],
    ) -> Result<()>;

    /// Update the parent tab reference for a tab
    async fn update_document_tab_parent(&self, id: &str, parent_tab_id: Option<&str>)
        -> Result<()>;

    /// Set current revision for a tab
    async fn set_document_tab_revision(&self, tab_id: &str, revision_id: &str) -> Result<()>;

    /// Delete a document tab
    async fn delete_document_tab(&self, id: &str) -> Result<bool>;

    // ========== Full Document Fetch Method =========

    /// Fetch the entire content of the document along with the tabs.
    async fn fetch_full_document(&self, doc_id: &str) -> Result<Option<FullDocumentInfo>> {
        let document = match self.get_document(doc_id).await? {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let tabs = self.list_document_tabs(doc_id).await?;

        Ok(Some(FullDocumentInfo { document, tabs }))
    }

    // ========== Document Revision Methods ==========

    /// Create a new revision for a tab
    async fn create_document_revision(
        &self,
        tab_id: &str,
        content_markdown: &str,
        content_hash: &str,
        referenced_assets: &[String],
        created_by: &str,
    ) -> Result<String>;

    /// Get a revision by ID
    async fn get_document_revision(&self, id: &str) -> Result<Option<DocumentRevisionInfo>>;

    /// List revisions for a tab
    async fn list_document_revisions(&self, tab_id: &str) -> Result<Vec<DocumentRevisionInfo>>;
}
