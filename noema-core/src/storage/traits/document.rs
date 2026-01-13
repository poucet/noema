//! DocumentStore trait for document, tab, and revision storage

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{AssetId, DocumentId, RevisionId, TabId, UserId};
use crate::storage::types::{Document, DocumentRevision, DocumentSource, DocumentTab, Stored, StoredEditable};

/// Trait for document storage operations
#[async_trait]
pub trait DocumentStore: Send + Sync {
    // ========== Document Methods ==========

    /// Create a new document
    async fn create_document(
        &self,
        user_id: &UserId,
        title: &str,
        source: DocumentSource,
        source_id: Option<&str>,
    ) -> Result<DocumentId>;

    /// Get a document by ID
    async fn get_document(&self, id: &DocumentId) -> Result<Option<StoredEditable<DocumentId, Document>>>;

    /// Get a document by source and source_id (e.g., find by Google Doc ID)
    async fn get_document_by_source(
        &self,
        user_id: &UserId,
        source: DocumentSource,
        source_id: &str,
    ) -> Result<Option<StoredEditable<DocumentId, Document>>>;

    /// List all documents for a user
    async fn list_documents(&self, user_id: &UserId) -> Result<Vec<StoredEditable<DocumentId, Document>>>;

    /// Search documents by title (case-insensitive)
    async fn search_documents(
        &self,
        user_id: &UserId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<StoredEditable<DocumentId, Document>>>;

    /// Update document title
    async fn update_document_title(&self, id: &DocumentId, title: &str) -> Result<()>;

    /// Delete a document and all its tabs/revisions
    async fn delete_document(&self, id: &DocumentId) -> Result<bool>;

    // ========== Document Tab Methods ==========

    /// Create a new document tab
    async fn create_document_tab(
        &self,
        document_id: &DocumentId,
        parent_tab_id: Option<&TabId>,
        tab_index: i32,
        title: &str,
        icon: Option<&str>,
        content_markdown: Option<&str>,
        referenced_assets: &[AssetId],
        source_tab_id: Option<&TabId>,
    ) -> Result<TabId>;

    /// Get a document tab by ID
    async fn get_document_tab(&self, id: &TabId) -> Result<Option<StoredEditable<TabId, DocumentTab>>>;

    /// List all tabs for a document
    async fn list_document_tabs(&self, document_id: &DocumentId) -> Result<Vec<StoredEditable<TabId, DocumentTab>>>;

    /// Update tab content
    async fn update_document_tab_content(
        &self,
        id: &TabId,
        content_markdown: &str,
        referenced_assets: &[AssetId],
    ) -> Result<()>;

    /// Update the parent tab reference for a tab
    async fn update_document_tab_parent(
        &self,
        id: &TabId,
        parent_tab_id: Option<&TabId>,
    ) -> Result<()>;

    /// Set current revision for a tab
    async fn set_document_tab_revision(&self, tab_id: &TabId, revision_id: &RevisionId)
        -> Result<()>;

    /// Delete a document tab
    async fn delete_document_tab(&self, id: &TabId) -> Result<bool>;

    // ========== Document Revision Methods ==========

    /// Create a new revision for a tab
    async fn create_document_revision(
        &self,
        tab_id: &TabId,
        content_markdown: &str,
        content_hash: &str,
        referenced_assets: &[AssetId],
        created_by: &UserId,
    ) -> Result<RevisionId>;

    /// Get a revision by ID
    async fn get_document_revision(&self, id: &RevisionId) -> Result<Option<Stored<RevisionId, DocumentRevision>>>;

    /// List revisions for a tab
    async fn list_document_revisions(&self, tab_id: &TabId) -> Result<Vec<Stored<RevisionId, DocumentRevision>>>;
}
