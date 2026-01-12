//! In-memory DocumentStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::traits::DocumentStore;
use crate::storage::types::document::{
    DocumentInfo, DocumentRevisionInfo, DocumentSource, DocumentTabInfo,
};

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// In-memory document store for testing
#[derive(Debug, Default)]
pub struct MemoryDocumentStore {
    documents: Mutex<HashMap<String, DocumentInfo>>,
    tabs: Mutex<HashMap<String, DocumentTabInfo>>,
    revisions: Mutex<HashMap<String, DocumentRevisionInfo>>,
}

impl MemoryDocumentStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DocumentStore for MemoryDocumentStore {
    // ========== Document Methods ==========

    async fn create_document(
        &self,
        user_id: &str,
        title: &str,
        source: DocumentSource,
        source_id: Option<&str>,
    ) -> Result<String> {
        let id = new_id();
        let now = now();

        let doc = DocumentInfo {
            id: id.clone(),
            user_id: user_id.to_string(),
            title: title.to_string(),
            source,
            source_id: source_id.map(|s| s.to_string()),
            created_at: now,
            updated_at: now,
        };

        self.documents.lock().unwrap().insert(id.clone(), doc);
        Ok(id)
    }

    async fn get_document(&self, id: &str) -> Result<Option<DocumentInfo>> {
        Ok(self.documents.lock().unwrap().get(id).cloned())
    }

    async fn get_document_by_source(
        &self,
        user_id: &str,
        source: DocumentSource,
        source_id: &str,
    ) -> Result<Option<DocumentInfo>> {
        let documents = self.documents.lock().unwrap();
        Ok(documents
            .values()
            .find(|d| {
                d.user_id == user_id
                    && d.source == source
                    && d.source_id.as_deref() == Some(source_id)
            })
            .cloned())
    }

    async fn list_documents(&self, user_id: &str) -> Result<Vec<DocumentInfo>> {
        let documents = self.documents.lock().unwrap();
        Ok(documents
            .values()
            .filter(|d| d.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn search_documents(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<DocumentInfo>> {
        let documents = self.documents.lock().unwrap();
        let query_lower = query.to_lowercase();
        let mut results: Vec<_> = documents
            .values()
            .filter(|d| d.user_id == user_id && d.title.to_lowercase().contains(&query_lower))
            .cloned()
            .collect();
        results.truncate(limit);
        Ok(results)
    }

    async fn update_document_title(&self, id: &str, title: &str) -> Result<()> {
        if let Some(doc) = self.documents.lock().unwrap().get_mut(id) {
            doc.title = title.to_string();
            doc.updated_at = now();
        }
        Ok(())
    }

    async fn delete_document(&self, id: &str) -> Result<bool> {
        // Delete associated tabs and revisions
        {
            let tabs = self.tabs.lock().unwrap();
            let tab_ids: Vec<_> = tabs
                .values()
                .filter(|t| t.document_id == id)
                .map(|t| t.id.clone())
                .collect();

            drop(tabs);

            let mut revisions = self.revisions.lock().unwrap();
            revisions.retain(|_, r| {
                !tab_ids.iter().any(|tid| r.tab_id == *tid)
            });

            let mut tabs = self.tabs.lock().unwrap();
            tabs.retain(|_, t| t.document_id != id);
        }

        Ok(self.documents.lock().unwrap().remove(id).is_some())
    }

    // ========== Document Tab Methods ==========

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
    ) -> Result<String> {
        let id = new_id();
        let now = now();

        let tab = DocumentTabInfo {
            id: id.clone(),
            document_id: document_id.to_string(),
            parent_tab_id: parent_tab_id.map(|s| s.to_string()),
            tab_index,
            title: title.to_string(),
            icon: icon.map(|s| s.to_string()),
            content_markdown: content_markdown.map(|s| s.to_string()),
            referenced_assets: referenced_assets.to_vec(),
            source_tab_id: source_tab_id.map(|s| s.to_string()),
            current_revision_id: None,
            created_at: now,
            updated_at: now,
        };

        self.tabs.lock().unwrap().insert(id.clone(), tab);

        // Update document updated_at
        if let Some(doc) = self.documents.lock().unwrap().get_mut(document_id) {
            doc.updated_at = now;
        }

        Ok(id)
    }

    async fn get_document_tab(&self, id: &str) -> Result<Option<DocumentTabInfo>> {
        Ok(self.tabs.lock().unwrap().get(id).cloned())
    }

    async fn list_document_tabs(&self, document_id: &str) -> Result<Vec<DocumentTabInfo>> {
        let tabs = self.tabs.lock().unwrap();
        let mut result: Vec<_> = tabs
            .values()
            .filter(|t| t.document_id == document_id)
            .cloned()
            .collect();
        result.sort_by_key(|t| t.tab_index);
        Ok(result)
    }

    async fn update_document_tab_content(
        &self,
        id: &str,
        content_markdown: &str,
        referenced_assets: &[String],
    ) -> Result<()> {
        let now = now();
        let mut tabs = self.tabs.lock().unwrap();
        if let Some(tab) = tabs.get_mut(id) {
            tab.content_markdown = Some(content_markdown.to_string());
            tab.referenced_assets = referenced_assets.to_vec();
            tab.updated_at = now;

            // Update document updated_at
            let doc_id = tab.document_id.clone();
            drop(tabs);
            if let Some(doc) = self.documents.lock().unwrap().get_mut(&doc_id) {
                doc.updated_at = now;
            }
        }
        Ok(())
    }

    async fn update_document_tab_parent(
        &self,
        id: &str,
        parent_tab_id: Option<&str>,
    ) -> Result<()> {
        if let Some(tab) = self.tabs.lock().unwrap().get_mut(id) {
            tab.parent_tab_id = parent_tab_id.map(|s| s.to_string());
            tab.updated_at = now();
        }
        Ok(())
    }

    async fn set_document_tab_revision(&self, tab_id: &str, revision_id: &str) -> Result<()> {
        if let Some(tab) = self.tabs.lock().unwrap().get_mut(tab_id) {
            tab.current_revision_id = Some(revision_id.to_string());
            tab.updated_at = now();
        }
        Ok(())
    }

    async fn delete_document_tab(&self, id: &str) -> Result<bool> {
        // Delete associated revisions
        self.revisions
            .lock()
            .unwrap()
            .retain(|_, r| r.tab_id != id);

        Ok(self.tabs.lock().unwrap().remove(id).is_some())
    }

    // ========== Document Revision Methods ==========

    async fn create_document_revision(
        &self,
        tab_id: &str,
        content_markdown: &str,
        content_hash: &str,
        referenced_assets: &[String],
        created_by: &str,
    ) -> Result<String> {
        let id = new_id();
        let now = now();

        // Get next revision number
        let revisions = self.revisions.lock().unwrap();
        let revision_number = revisions
            .values()
            .filter(|r| r.tab_id == tab_id)
            .map(|r| r.revision_number)
            .max()
            .map(|n| n + 1)
            .unwrap_or(1);

        // Get parent revision
        let parent_revision_id = revisions
            .values()
            .filter(|r| r.tab_id == tab_id)
            .max_by_key(|r| r.revision_number)
            .map(|r| r.id.clone());
        drop(revisions);

        let revision = DocumentRevisionInfo {
            id: id.clone(),
            tab_id: tab_id.to_string(),
            revision_number,
            parent_revision_id,
            content_markdown: content_markdown.to_string(),
            content_hash: content_hash.to_string(),
            referenced_assets: referenced_assets.to_vec(),
            created_at: now,
            created_by: created_by.to_string(),
        };

        self.revisions.lock().unwrap().insert(id.clone(), revision);

        Ok(id)
    }

    async fn get_document_revision(&self, id: &str) -> Result<Option<DocumentRevisionInfo>> {
        Ok(self.revisions.lock().unwrap().get(id).cloned())
    }

    async fn list_document_revisions(&self, tab_id: &str) -> Result<Vec<DocumentRevisionInfo>> {
        let revisions = self.revisions.lock().unwrap();
        let mut result: Vec<_> = revisions
            .values()
            .filter(|r| r.tab_id == tab_id)
            .cloned()
            .collect();
        result.sort_by_key(|r| r.revision_number);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_document_crud() {
        let store = MemoryDocumentStore::new();

        // Create document
        let doc_id = store
            .create_document("user1", "Test Doc", DocumentSource::UserCreated, None)
            .await
            .unwrap();

        // Get document
        let doc = store.get_document(&doc_id).await.unwrap().unwrap();
        assert_eq!(doc.title, "Test Doc");
        assert_eq!(doc.source, DocumentSource::UserCreated);

        // Update title
        store
            .update_document_title(&doc_id, "Updated Title")
            .await
            .unwrap();
        let doc = store.get_document(&doc_id).await.unwrap().unwrap();
        assert_eq!(doc.title, "Updated Title");

        // Delete
        assert!(store.delete_document(&doc_id).await.unwrap());
        assert!(store.get_document(&doc_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_document_tabs() {
        let store = MemoryDocumentStore::new();

        // Create document
        let doc_id = store
            .create_document("user1", "Test Doc", DocumentSource::UserCreated, None)
            .await
            .unwrap();

        // Create tabs
        let tab1_id = store
            .create_document_tab(&doc_id, None, 0, "Tab 1", None, Some("# Tab 1"), &[], None)
            .await
            .unwrap();

        let tab2_id = store
            .create_document_tab(&doc_id, None, 1, "Tab 2", None, Some("# Tab 2"), &[], None)
            .await
            .unwrap();

        // List tabs
        let tabs = store.list_document_tabs(&doc_id).await.unwrap();
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].title, "Tab 1");
        assert_eq!(tabs[1].title, "Tab 2");

        // Update tab content
        store
            .update_document_tab_content(&tab1_id, "# Updated Tab 1", &["asset1".to_string()])
            .await
            .unwrap();

        let tab = store.get_document_tab(&tab1_id).await.unwrap().unwrap();
        assert_eq!(tab.content_markdown, Some("# Updated Tab 1".to_string()));
        assert_eq!(tab.referenced_assets, vec!["asset1".to_string()]);

        // Delete tab
        assert!(store.delete_document_tab(&tab1_id).await.unwrap());
        let tabs = store.list_document_tabs(&doc_id).await.unwrap();
        assert_eq!(tabs.len(), 1);
    }

    #[tokio::test]
    async fn test_document_revisions() {
        let store = MemoryDocumentStore::new();

        // Create document and tab
        let doc_id = store
            .create_document("user1", "Test Doc", DocumentSource::UserCreated, None)
            .await
            .unwrap();

        let tab_id = store
            .create_document_tab(&doc_id, None, 0, "Tab 1", None, Some("# Tab 1"), &[], None)
            .await
            .unwrap();

        // Create revisions
        let rev1_id = store
            .create_document_revision(&tab_id, "# Version 1", "hash1", &[], "user1")
            .await
            .unwrap();

        let rev2_id = store
            .create_document_revision(&tab_id, "# Version 2", "hash2", &[], "user1")
            .await
            .unwrap();

        // List revisions
        let revisions = store.list_document_revisions(&tab_id).await.unwrap();
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[0].revision_number, 1);
        assert_eq!(revisions[1].revision_number, 2);
        assert_eq!(
            revisions[1].parent_revision_id,
            Some(rev1_id.clone())
        );

        // Get revision
        let rev = store.get_document_revision(&rev2_id).await.unwrap().unwrap();
        assert_eq!(rev.content_markdown, "# Version 2");
    }

    #[tokio::test]
    async fn test_search_documents() {
        let store = MemoryDocumentStore::new();

        // Create documents
        store
            .create_document("user1", "Meeting Notes", DocumentSource::UserCreated, None)
            .await
            .unwrap();
        store
            .create_document("user1", "Project Plan", DocumentSource::UserCreated, None)
            .await
            .unwrap();
        store
            .create_document("user1", "Meeting Summary", DocumentSource::UserCreated, None)
            .await
            .unwrap();

        // Search
        let results = store
            .search_documents("user1", "meeting", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_get_document_by_source() {
        let store = MemoryDocumentStore::new();

        // Create document with source_id
        let doc_id = store
            .create_document(
                "user1",
                "Google Doc",
                DocumentSource::GoogleDrive,
                Some("gdoc123"),
            )
            .await
            .unwrap();

        // Find by source
        let found = store
            .get_document_by_source("user1", DocumentSource::GoogleDrive, "gdoc123")
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, doc_id);

        // Not found
        let not_found = store
            .get_document_by_source("user1", DocumentSource::GoogleDrive, "nonexistent")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }
}
