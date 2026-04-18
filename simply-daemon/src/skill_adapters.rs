//! Adapters that bridge daemon services to skill trait objects.
//!
//! Skills define their own trait objects (e.g., `SkillDocumentApi`) to avoid
//! depending on the daemon. These adapters implement those traits using the
//! daemon's actual services.

use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;

use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::ids::UserId;
use simply_core::storage::traits::{DocumentStore, StorageTypes, Stores};
use simply_core::storage::types::DocumentType;
use simply_core::storage::DocumentSource;

/// Adapter: daemon's DocumentStore → GDocsSkill's SkillDocumentApi.
pub struct DocumentApiAdapter<S: StorageTypes> {
    stores: Arc<dyn Stores<S>>,
}

impl<S: StorageTypes> DocumentApiAdapter<S> {
    pub fn new(stores: Arc<dyn Stores<S>>) -> Self {
        Self { stores }
    }
}

#[async_trait]
impl<S: StorageTypes> mcp_gdocs::skill::SkillDocumentApi for DocumentApiAdapter<S> {
    async fn create_document(&self, user_id: &UserId, title: &str, source_id: Option<&str>) -> Result<String> {
        let doc_id = self.stores.document()
            .create_document(user_id, title, DocumentType::KNOWLEDGE, DocumentSource::GoogleDrive, source_id)
            .await?;
        Ok(doc_id.as_str().to_string())
    }

    async fn create_tab(&self, doc_id: &str, title: &str, content: &str, parent_tab_id: Option<&str>, tab_index: i32) -> Result<String> {
        let doc_id = simply_core::storage::ids::DocumentId::from_string(doc_id);
        let parent = parent_tab_id.map(simply_core::storage::ids::TabId::from_string);
        let tab_id = self.stores.document()
            .create_document_tab(
                &doc_id,
                parent.as_ref(),
                tab_index,
                title,
                None, // icon
                Some(content),
                &[], // referenced_assets
                None, // source_tab_id
            )
            .await?;
        Ok(tab_id.as_str().to_string())
    }

    async fn find_by_source(&self, user_id: &UserId, source_id: &str) -> Result<Option<String>> {
        let doc = self.stores.document()
            .get_document_by_source(user_id, DocumentSource::GoogleDrive, source_id)
            .await?;
        Ok(doc.map(|d| d.id.as_str().to_string()))
    }
}

/// Adapter: daemon's StorageCoordinator → GDocsSkill's SkillAssetApi.
pub struct AssetApiAdapter<S: StorageTypes> {
    coordinator: Arc<StorageCoordinator<S>>,
}

impl<S: StorageTypes> AssetApiAdapter<S> {
    pub fn new(coordinator: Arc<StorageCoordinator<S>>) -> Self {
        Self { coordinator }
    }
}

#[async_trait]
impl<S: StorageTypes> mcp_gdocs::skill::SkillAssetApi for AssetApiAdapter<S> {
    async fn store_asset(&self, base64_data: &str, mime_type: &str) -> Result<String> {
        let asset_id = self.coordinator.store_asset(base64_data, mime_type).await?;
        Ok(asset_id.into_string())
    }
}
