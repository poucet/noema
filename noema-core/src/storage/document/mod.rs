//! Document storage trait and implementations
//!
//! Provides the `DocumentStore` trait for managing documents, tabs, and revisions.
//! Compatible with the Episteme document model.

use anyhow::Result;
use async_trait::async_trait;
use std::str::FromStr;

// ============================================================================
// Types
// ============================================================================

/// Document source type (matches episteme)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSource {
    GoogleDrive,
    AiGenerated,
    UserCreated,
}

impl ToString for DocumentSource {
    fn to_string(&self) -> String {
        match self {
            DocumentSource::GoogleDrive => "google_drive".to_string(),
            DocumentSource::AiGenerated => "ai_generated".to_string(),
            DocumentSource::UserCreated => "user_created".to_string(),
        }
    }
}

impl FromStr for DocumentSource {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "google_drive" => Ok(DocumentSource::GoogleDrive),
            "ai_generated" => Ok(DocumentSource::AiGenerated),
            "user_created" => Ok(DocumentSource::UserCreated),
            _ => Err(format!("{s} is not a valid DocumentSource")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocumentInfo {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub source: DocumentSource,
    pub source_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct DocumentTabInfo {
    pub id: String,
    pub document_id: String,
    pub parent_tab_id: Option<String>,
    pub tab_index: i32,
    pub title: String,
    pub icon: Option<String>,
    pub content_markdown: Option<String>,
    pub referenced_assets: Vec<String>,
    pub source_tab_id: Option<String>,
    pub current_revision_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct DocumentRevisionInfo {
    pub id: String,
    pub tab_id: String,
    pub revision_number: i32,
    pub parent_revision_id: Option<String>,
    pub content_markdown: String,
    pub content_hash: String,
    pub referenced_assets: Vec<String>,
    pub created_at: i64,
    pub created_by: String,
}

#[derive(Debug, Clone)]
pub struct FullDocumentInfo {
    pub document: DocumentInfo,
    pub tabs: Vec<DocumentTabInfo>,
}
// ============================================================================
// Trait
// ============================================================================

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

pub mod resolver;

#[cfg(feature = "sqlite")]
pub (crate) mod sqlite;
