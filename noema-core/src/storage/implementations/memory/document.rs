//! In-memory DocumentStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::ids::{AssetId, DocumentId, MessageId, RevisionId, TabId, UserId};
use crate::storage::traits::DocumentStore;
use crate::storage::types::{
    stored, stored_editable, Document, DocumentRevision, DocumentSource, DocumentTab,
    Stored, StoredEditable,
};

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// In-memory document store for testing
#[derive(Debug, Default)]
pub struct MemoryDocumentStore {
    documents: Mutex<HashMap<String, StoredEditable<DocumentId, Document>>>,
    tabs: Mutex<HashMap<String, StoredEditable<TabId, DocumentTab>>>,
    revisions: Mutex<HashMap<String, Stored<RevisionId, DocumentRevision>>>,
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
        user_id: &UserId,
        title: &str,
        source: DocumentSource,
        source_id: Option<&str>,
    ) -> Result<DocumentId> {
        let id = DocumentId::new();
        let now = now();

        let doc = Document {
            user_id: user_id.clone(),
            title: title.to_string(),
            source,
            source_id: source_id.map(|s| s.to_string()),
        };

        let stored = stored_editable(id.clone(), doc, now, now);
        self.documents.lock().unwrap().insert(id.as_str().to_string(), stored);
        Ok(id)
    }

    async fn get_document(&self, id: &DocumentId) -> Result<Option<StoredEditable<DocumentId, Document>>> {
        Ok(self.documents.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn get_document_by_source(
        &self,
        user_id: &UserId,
        source: DocumentSource,
        source_id: &str,
    ) -> Result<Option<StoredEditable<DocumentId, Document>>> {
        let documents = self.documents.lock().unwrap();
        Ok(documents
            .values()
            .find(|d| {
                d.user_id == *user_id
                    && d.source == source
                    && d.source_id.as_deref() == Some(source_id)
            })
            .cloned())
    }

    async fn list_documents(&self, user_id: &UserId) -> Result<Vec<StoredEditable<DocumentId, Document>>> {
        let documents = self.documents.lock().unwrap();
        Ok(documents
            .values()
            .filter(|d| d.user_id == *user_id)
            .cloned()
            .collect())
    }

    async fn search_documents(
        &self,
        user_id: &UserId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<StoredEditable<DocumentId, Document>>> {
        let documents = self.documents.lock().unwrap();
        let query_lower = query.to_lowercase();
        let mut results: Vec<_> = documents
            .values()
            .filter(|d| d.user_id == *user_id && d.title.to_lowercase().contains(&query_lower))
            .cloned()
            .collect();
        results.truncate(limit);
        Ok(results)
    }

    async fn update_document_title(&self, id: &DocumentId, title: &str) -> Result<()> {
        if let Some(doc) = self.documents.lock().unwrap().get_mut(id.as_str()) {
            doc.title = title.to_string();
            doc.updated_at = now();
        }
        Ok(())
    }

    async fn delete_document(&self, id: &DocumentId) -> Result<bool> {
        // Delete associated tabs and revisions
        {
            let tabs = self.tabs.lock().unwrap();
            let tab_ids: Vec<_> = tabs
                .values()
                .filter(|t| t.document_id == *id)
                .map(|t| t.id.as_str().to_string())
                .collect();

            drop(tabs);

            let mut revisions = self.revisions.lock().unwrap();
            revisions.retain(|_, r| {
                !tab_ids.iter().any(|tid| r.tab_id.as_str() == tid)
            });

            let mut tabs = self.tabs.lock().unwrap();
            tabs.retain(|_, t| t.document_id != *id);
        }

        Ok(self.documents.lock().unwrap().remove(id.as_str()).is_some())
    }

    // ========== Document Tab Methods ==========

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
    ) -> Result<TabId> {
        let id = TabId::new();
        let now = now();

        let tab = DocumentTab {
            document_id: document_id.clone(),
            parent_tab_id: parent_tab_id.cloned(),
            tab_index,
            title: title.to_string(),
            icon: icon.map(|s| s.to_string()),
            content_markdown: content_markdown.map(|s| s.to_string()),
            referenced_assets: referenced_assets.to_vec(),
            source_tab_id: source_tab_id.cloned(),
            current_revision_id: None,
        };

        let stored = stored_editable(id.clone(), tab, now, now);
        self.tabs.lock().unwrap().insert(id.as_str().to_string(), stored);

        // Update document updated_at
        if let Some(doc) = self.documents.lock().unwrap().get_mut(document_id.as_str()) {
            doc.content.updated_at = now;
        }

        Ok(id)
    }

    async fn get_document_tab(&self, id: &TabId) -> Result<Option<StoredEditable<TabId, DocumentTab>>> {
        Ok(self.tabs.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn list_document_tabs(&self, document_id: &DocumentId) -> Result<Vec<StoredEditable<TabId, DocumentTab>>> {
        let tabs = self.tabs.lock().unwrap();
        let mut result: Vec<_> = tabs
            .values()
            .filter(|t| t.document_id == *document_id)
            .cloned()
            .collect();
        result.sort_by_key(|t| t.tab_index);
        Ok(result)
    }

    async fn update_document_tab_content(
        &self,
        id: &TabId,
        content_markdown: &str,
        referenced_assets: &[AssetId],
    ) -> Result<()> {
        let now = now();
        let mut tabs = self.tabs.lock().unwrap();
        if let Some(tab) = tabs.get_mut(id.as_str()) {
            tab.content_markdown = Some(content_markdown.to_string());
            tab.referenced_assets = referenced_assets.to_vec();
            tab.updated_at = now;

            // Update document updated_at
            let doc_id = tab.document_id.as_str().to_string();
            drop(tabs);
            if let Some(doc) = self.documents.lock().unwrap().get_mut(&doc_id) {
                doc.updated_at = now;
            }
        }
        Ok(())
    }

    async fn update_document_tab_parent(
        &self,
        id: &TabId,
        parent_tab_id: Option<&TabId>,
    ) -> Result<()> {
        if let Some(tab) = self.tabs.lock().unwrap().get_mut(id.as_str()) {
            tab.parent_tab_id = parent_tab_id.cloned();
            tab.updated_at = now();
        }
        Ok(())
    }

    async fn set_document_tab_revision(&self, tab_id: &TabId, revision_id: &RevisionId) -> Result<()> {
        if let Some(tab) = self.tabs.lock().unwrap().get_mut(tab_id.as_str()) {
            tab.current_revision_id = Some(revision_id.clone());
            tab.updated_at = now();
        }
        Ok(())
    }

    async fn delete_document_tab(&self, id: &TabId) -> Result<bool> {
        // Delete associated revisions
        self.revisions
            .lock()
            .unwrap()
            .retain(|_, r| r.tab_id != *id);

        Ok(self.tabs.lock().unwrap().remove(id.as_str()).is_some())
    }

    // ========== Document Revision Methods ==========

    async fn create_document_revision(
        &self,
        tab_id: &TabId,
        content_markdown: &str,
        content_hash: &str,
        referenced_assets: &[AssetId],
        created_by: &UserId,
    ) -> Result<RevisionId> {
        let id = RevisionId::new();
        let now = now();

        // Get next revision number
        let revisions = self.revisions.lock().unwrap();
        let revision_number = revisions
            .values()
            .filter(|r| r.tab_id == *tab_id)
            .map(|r| r.revision_number)
            .max()
            .map(|n| n + 1)
            .unwrap_or(1);

        // Get parent revision
        let parent_revision_id = revisions
            .values()
            .filter(|r| r.tab_id == *tab_id)
            .max_by_key(|r| r.revision_number)
            .map(|r| r.id.clone());
        drop(revisions);

        let revision = DocumentRevision {
            tab_id: tab_id.clone(),
            revision_number,
            parent_revision_id,
            content_markdown: content_markdown.to_string(),
            content_hash: content_hash.to_string(),
            referenced_assets: referenced_assets.to_vec(),
            created_by: created_by.clone(),
        };

        let stored = stored(id.clone(), revision, now);
        self.revisions.lock().unwrap().insert(id.as_str().to_string(), stored);

        Ok(id)
    }

    async fn get_document_revision(&self, id: &RevisionId) -> Result<Option<Stored<RevisionId, DocumentRevision>>> {
        Ok(self.revisions.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn list_document_revisions(&self, tab_id: &TabId) -> Result<Vec<Stored<RevisionId, DocumentRevision>>> {
        let revisions = self.revisions.lock().unwrap();
        let mut result: Vec<_> = revisions
            .values()
            .filter(|r| r.tab_id == *tab_id)
            .cloned()
            .collect();
        result.sort_by_key(|r| r.revision_number);
        Ok(result)
    }

    async fn promote_from_message(
        &self,
        _message_id: &MessageId,
        _user_id: &UserId,
        _title: Option<&str>,
    ) -> Result<DocumentId> {
        anyhow::bail!("promote_from_message not supported in memory store (requires message content access)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_document_crud() {
        let store = MemoryDocumentStore::new();
        let user_id = UserId::new();

        // Create document
        let doc_id = store
            .create_document(&user_id, "Test Doc", DocumentSource::UserCreated, None)
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
        let user_id = UserId::new();

        // Create document
        let doc_id = store
            .create_document(&user_id, "Test Doc", DocumentSource::UserCreated, None)
            .await
            .unwrap();

        // Create tabs
        let tab1_id = store
            .create_document_tab(&doc_id, None, 0, "Tab 1", None, Some("# Tab 1"), &[], None)
            .await
            .unwrap();

        let _tab2_id = store
            .create_document_tab(&doc_id, None, 1, "Tab 2", None, Some("# Tab 2"), &[], None)
            .await
            .unwrap();

        // List tabs
        let tabs = store.list_document_tabs(&doc_id).await.unwrap();
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].title, "Tab 1");
        assert_eq!(tabs[1].title, "Tab 2");

        // Update tab content
        let asset_id = AssetId::from_string("asset1");
        store
            .update_document_tab_content(&tab1_id, "# Updated Tab 1", &[asset_id.clone()])
            .await
            .unwrap();

        let tab = store.get_document_tab(&tab1_id).await.unwrap().unwrap();
        assert_eq!(tab.content_markdown, Some("# Updated Tab 1".to_string()));
        assert_eq!(tab.referenced_assets, vec![asset_id]);

        // Delete tab
        assert!(store.delete_document_tab(&tab1_id).await.unwrap());
        let tabs = store.list_document_tabs(&doc_id).await.unwrap();
        assert_eq!(tabs.len(), 1);
    }

    #[tokio::test]
    async fn test_document_revisions() {
        let store = MemoryDocumentStore::new();
        let user_id = UserId::new();

        // Create document and tab
        let doc_id = store
            .create_document(&user_id, "Test Doc", DocumentSource::UserCreated, None)
            .await
            .unwrap();

        let tab_id = store
            .create_document_tab(&doc_id, None, 0, "Tab 1", None, Some("# Tab 1"), &[], None)
            .await
            .unwrap();

        // Create revisions
        let rev1_id = store
            .create_document_revision(&tab_id, "# Version 1", "hash1", &[], &user_id)
            .await
            .unwrap();

        let rev2_id = store
            .create_document_revision(&tab_id, "# Version 2", "hash2", &[], &user_id)
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
        let user_id = UserId::new();

        // Create documents
        store
            .create_document(&user_id, "Meeting Notes", DocumentSource::UserCreated, None)
            .await
            .unwrap();
        store
            .create_document(&user_id, "Project Plan", DocumentSource::UserCreated, None)
            .await
            .unwrap();
        store
            .create_document(&user_id, "Meeting Summary", DocumentSource::UserCreated, None)
            .await
            .unwrap();

        // Search
        let results = store
            .search_documents(&user_id, "meeting", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_get_document_by_source() {
        let store = MemoryDocumentStore::new();
        let user_id = UserId::new();

        // Create document with source_id
        let doc_id = store
            .create_document(
                &user_id,
                "Google Doc",
                DocumentSource::GoogleDrive,
                Some("gdoc123"),
            )
            .await
            .unwrap();

        // Find by source
        let found = store
            .get_document_by_source(&user_id, DocumentSource::GoogleDrive, "gdoc123")
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, doc_id);

        // Not found
        let not_found = store
            .get_document_by_source(&user_id, DocumentSource::GoogleDrive, "nonexistent")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }
}
