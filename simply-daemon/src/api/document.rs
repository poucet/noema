//! Document CRUD API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Document summary for listing.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[ts(export, export_to = "admin/src/lib/generated/")]
pub struct DocumentInfo {
    pub id: String,
    pub title: String,
    pub source: String,
    pub source_id: Option<String>,
    pub tab_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Full document with tabs.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[ts(export, export_to = "admin/src/lib/generated/")]
pub struct DocumentDetail {
    pub id: String,
    pub title: String,
    pub source: String,
    pub source_id: Option<String>,
    pub tabs: Vec<TabInfo>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Tab within a document.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[ts(export, export_to = "admin/src/lib/generated/")]
pub struct TabInfo {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub parent_tab_id: Option<String>,
    pub tab_index: i32,
    pub content_markdown: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Request to create a document.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[ts(export, export_to = "admin/src/lib/generated/")]
pub struct CreateDocumentRequest {
    pub title: String,
    /// Content for the initial tab (markdown).
    pub content: Option<String>,
}

/// Request to create a tab.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[ts(export, export_to = "admin/src/lib/generated/")]
pub struct CreateTabRequest {
    pub title: String,
    pub content: Option<String>,
    pub parent_tab_id: Option<String>,
    pub tab_index: Option<i32>,
}

/// Request to update a tab's content.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[ts(export, export_to = "admin/src/lib/generated/")]
pub struct UpdateTabRequest {
    pub content: String,
}

#[simply_rpc::rpc_service("document")]
#[async_trait]
pub trait DocumentApi: Send + Sync {
    /// List all documents.
    #[rpc(get = "/document")]
    async fn list_documents(&self) -> anyhow::Result<Vec<DocumentInfo>>;

    /// Search documents by title.
    #[rpc(get = "/document/search/{query}")]
    async fn search_documents(&self, query: &str) -> anyhow::Result<Vec<DocumentInfo>>;

    /// Get a document with all its tabs.
    #[rpc(get = "/document/{document_id}")]
    async fn get_document(&self, document_id: &str) -> anyhow::Result<DocumentDetail>;

    /// Create a new document (with optional initial content).
    #[rpc(post = "/document")]
    async fn create_document(&self, request: CreateDocumentRequest) -> anyhow::Result<DocumentInfo>;

    /// Update a document's title.
    #[rpc(put = "/document/{document_id}")]
    async fn rename_document(&self, document_id: &str, title: &str) -> anyhow::Result<()>;

    /// Delete a document and all its tabs.
    #[rpc(delete = "/document/{document_id}")]
    async fn delete_document(&self, document_id: &str) -> anyhow::Result<()>;

    /// Create a tab in a document.
    #[rpc(post = "/document/{document_id}/tab")]
    async fn create_tab(&self, document_id: &str, request: CreateTabRequest) -> anyhow::Result<TabInfo>;

    /// Get a tab's content.
    #[rpc(get = "/document/tab/{tab_id}")]
    async fn get_tab(&self, tab_id: &str) -> anyhow::Result<TabInfo>;

    /// Update a tab's content.
    #[rpc(put = "/document/tab/{tab_id}")]
    async fn update_tab(&self, tab_id: &str, request: UpdateTabRequest) -> anyhow::Result<()>;

    /// Delete a tab.
    #[rpc(delete = "/document/tab/{tab_id}")]
    async fn delete_tab(&self, tab_id: &str) -> anyhow::Result<()>;
}
