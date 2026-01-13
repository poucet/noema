//! Mock document store for testing

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{AssetId, DocumentId, RevisionId, TabId, UserId};
use crate::storage::traits::DocumentStore;
use crate::storage::types::{
    DocumentInfo, DocumentRevisionInfo, DocumentSource, DocumentTabInfo,
};

/// Mock document store that returns unimplemented for all operations
pub struct MockDocumentStore;

#[async_trait]
impl DocumentStore for MockDocumentStore {
    async fn create_document(
        &self,
        _: &UserId,
        _: &str,
        _: DocumentSource,
        _: Option<&str>,
    ) -> Result<DocumentId> {
        unimplemented!()
    }
    async fn get_document(&self, _: &DocumentId) -> Result<Option<DocumentInfo>> {
        unimplemented!()
    }
    async fn get_document_by_source(
        &self,
        _: &UserId,
        _: DocumentSource,
        _: &str,
    ) -> Result<Option<DocumentInfo>> {
        unimplemented!()
    }
    async fn list_documents(&self, _: &UserId) -> Result<Vec<DocumentInfo>> {
        unimplemented!()
    }
    async fn search_documents(
        &self,
        _: &UserId,
        _: &str,
        _: usize,
    ) -> Result<Vec<DocumentInfo>> {
        unimplemented!()
    }
    async fn update_document_title(&self, _: &DocumentId, _: &str) -> Result<()> {
        unimplemented!()
    }
    async fn delete_document(&self, _: &DocumentId) -> Result<bool> {
        unimplemented!()
    }
    async fn create_document_tab(
        &self,
        _: &DocumentId,
        _: Option<&TabId>,
        _: i32,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
        _: &[AssetId],
        _: Option<&TabId>,
    ) -> Result<TabId> {
        unimplemented!()
    }
    async fn get_document_tab(&self, _: &TabId) -> Result<Option<DocumentTabInfo>> {
        unimplemented!()
    }
    async fn list_document_tabs(&self, _: &DocumentId) -> Result<Vec<DocumentTabInfo>> {
        unimplemented!()
    }
    async fn update_document_tab_content(
        &self,
        _: &TabId,
        _: &str,
        _: &[AssetId],
    ) -> Result<()> {
        unimplemented!()
    }
    async fn update_document_tab_parent(&self, _: &TabId, _: Option<&TabId>) -> Result<()> {
        unimplemented!()
    }
    async fn set_document_tab_revision(&self, _: &TabId, _: &RevisionId) -> Result<()> {
        unimplemented!()
    }
    async fn delete_document_tab(&self, _: &TabId) -> Result<bool> {
        unimplemented!()
    }
    async fn create_document_revision(
        &self,
        _: &TabId,
        _: &str,
        _: &str,
        _: &[AssetId],
        _: &UserId,
    ) -> Result<RevisionId> {
        unimplemented!()
    }
    async fn get_document_revision(&self, _: &RevisionId) -> Result<Option<DocumentRevisionInfo>> {
        unimplemented!()
    }
    async fn list_document_revisions(&self, _: &TabId) -> Result<Vec<DocumentRevisionInfo>> {
        unimplemented!()
    }
}
