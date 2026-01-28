//! Google Docs commands for managing imported documents
//!
//! Uses the episteme-compatible document model with documents, tabs, and revisions.
//! Google Drive/Docs API calls are made directly using GoogleDocsClient.

use base64::Engine as _;
use config::PathManager;
use noema_core::storage::ids::{AssetId, DocumentId, RevisionId, TabId, UserId};
use noema_core::storage::{Document, DocumentSource, DocumentStore, DocumentTab, StoredEditable, Stores, UserStore};
use noema_mcp_gdocs::GoogleDocsClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tracing::{debug, info};
use ts_rs::TS;

use crate::state::AppState;

// ============================================================================
// Google OAuth Token Storage
// ============================================================================

/// Stored Google OAuth credentials and tokens
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoogleOAuthConfig {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

impl GoogleOAuthConfig {
    fn config_path() -> Option<std::path::PathBuf> {
        PathManager::data_dir().map(|d| d.join("google_oauth.json"))
    }

    pub fn load() -> Option<Self> {
        let path = Self::config_path()?;
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path().ok_or("Could not determine data directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }

    pub fn has_credentials(&self) -> bool {
        !self.client_id.is_empty()
    }

    pub fn has_valid_token(&self) -> bool {
        self.access_token.is_some()
    }

    /// Create a GoogleDocsClient if we have a valid access token
    pub fn create_client(&self) -> Option<GoogleDocsClient> {
        self.access_token.as_ref().map(|token| GoogleDocsClient::new(token.clone()))
    }
}

/// Document info response for the frontend
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct DocumentInfoResponse {
    #[ts(type = "string")]
    pub id: DocumentId,
    #[ts(type = "string")]
    pub user_id: UserId,
    pub title: String,
    pub source: String,
    pub source_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<StoredEditable<DocumentId, Document>> for DocumentInfoResponse {
    fn from(info: StoredEditable<DocumentId, Document>) -> Self {
        DocumentInfoResponse {
            id: info.id.clone(),
            user_id: info.user_id.clone(),
            title: info.title.clone(),
            source: info.source.to_string(),
            source_id: info.source_id.clone(),
            created_at: info.created_at(),
            updated_at: info.updated_at(),
        }
    }
}

/// Document tab response for the frontend
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct DocumentTabResponse {
    #[ts(type = "string")]
    pub id: TabId,
    #[ts(type = "string")]
    pub document_id: DocumentId,
    #[ts(type = "string | null")]
    pub parent_tab_id: Option<TabId>,
    pub tab_index: i32,
    pub title: String,
    pub icon: Option<String>,
    pub content_markdown: Option<String>,
    #[ts(type = "string[]")]
    pub referenced_assets: Vec<AssetId>,
    #[ts(type = "string | null")]
    pub source_tab_id: Option<TabId>,
    #[ts(type = "string | null")]
    pub current_revision_id: Option<RevisionId>, // RevisionId as string for simplicity
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<StoredEditable<TabId, DocumentTab>> for DocumentTabResponse {
    fn from(tab: StoredEditable<TabId, DocumentTab>) -> Self {
        DocumentTabResponse {
            id: tab.id.clone(),
            document_id: tab.document_id.clone(),
            parent_tab_id: tab.parent_tab_id.clone(),
            tab_index: tab.tab_index,
            title: tab.title.clone(),
            icon: tab.icon.clone(),
            content_markdown: tab.content_markdown.clone(),
            referenced_assets: tab.referenced_assets.clone(),
            source_tab_id: tab.source_tab_id.clone(),
            current_revision_id: tab.current_revision_id.clone(),
            created_at: tab.created_at,
            updated_at: tab.updated_at,
        }
    }
}

/// Full document content response with all tabs
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct DocumentContentResponse {
    pub info: DocumentInfoResponse,
    pub tabs: Vec<DocumentTabResponse>,
}

/// List all documents for the current user
#[tauri::command]
pub async fn list_documents(state: State<'_, Arc<AppState>>) -> Result<Vec<DocumentInfoResponse>, String> {
    let stores = state.get_stores()?;

    // Get default user
    let user = stores.user()
        .get_or_create_default_user()
        .await
        .map_err(|e| e.to_string())?;

    let docs = stores.document()
        .list_documents(&user.id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(docs.into_iter().map(DocumentInfoResponse::from).collect())
}

/// Get a single document by ID
#[tauri::command]
pub async fn get_document(
    state: State<'_, Arc<AppState>>,
    doc_id: DocumentId,
) -> Result<Option<DocumentInfoResponse>, String> {
    let stores = state.get_stores()?;

    let doc = stores.document().get_document(&doc_id).await.map_err(|e| e.to_string())?;
    Ok(doc.map(DocumentInfoResponse::from))
}

/// Get document by Google Doc ID (source_id)
#[tauri::command]
pub async fn get_document_by_google_id(
    state: State<'_, Arc<AppState>>,
    google_doc_id: String,
) -> Result<Option<DocumentInfoResponse>, String> {
    let stores = state.get_stores()?;

    let user = stores.user()
        .get_or_create_default_user()
        .await
        .map_err(|e| e.to_string())?;

    let doc = stores.document()
        .get_document_by_source(&user.id, DocumentSource::GoogleDrive, &google_doc_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(doc.map(DocumentInfoResponse::from))
}

/// Get document content with all tabs
#[tauri::command]
pub async fn get_document_content(
    state: State<'_, Arc<AppState>>,
    doc_id: DocumentId,
) -> Result<DocumentContentResponse, String> {
    let stores = state.get_stores()?;
    let document_store = stores.document();

    // Get document metadata
    let doc = document_store
        .get_document(&doc_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Document not found")?;

    // Get all tabs for this document
    let tabs = document_store
        .list_document_tabs(&doc_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(DocumentContentResponse {
        info: DocumentInfoResponse::from(doc),
        tabs: tabs.into_iter().map(DocumentTabResponse::from).collect(),
    })
}

/// Get a single tab's content
#[tauri::command]
pub async fn get_document_tab(
    state: State<'_, Arc<AppState>>,
    tab_id: TabId,
) -> Result<Option<DocumentTabResponse>, String> {
    let stores = state.get_stores()?;

    let tab = stores.document()
        .get_document_tab(&tab_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(tab.map(DocumentTabResponse::from))
}

/// Delete a document and all its tabs/revisions
#[tauri::command]
pub async fn delete_document(state: State<'_, Arc<AppState>>, doc_id: DocumentId) -> Result<bool, String> {
    let stores = state.get_stores()?;

    stores.document().delete_document(&doc_id).await.map_err(|e| e.to_string())
}

/// Create a new user document
#[tauri::command]
pub async fn create_document(
    state: State<'_, Arc<AppState>>,
    title: String,
) -> Result<DocumentInfoResponse, String> {
    let stores = state.get_stores()?;
    let user_id = state.user_id.lock().await.clone();

    let doc_id = stores
        .document()
        .create_document(&user_id, &title, DocumentSource::UserCreated, None)
        .await
        .map_err(|e| e.to_string())?;

    let doc = stores
        .document()
        .get_document(&doc_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Document not found after creation")?;

    Ok(DocumentInfoResponse::from(doc))
}

/// Update a document's title
#[tauri::command]
pub async fn update_document_title(
    state: State<'_, Arc<AppState>>,
    doc_id: DocumentId,
    title: String,
) -> Result<(), String> {
    let stores = state.get_stores()?;

    stores
        .document()
        .update_document_title(&doc_id, &title)
        .await
        .map_err(|e| e.to_string())
}

/// Create a new tab in a document
#[tauri::command]
pub async fn create_document_tab(
    state: State<'_, Arc<AppState>>,
    doc_id: DocumentId,
    title: String,
    parent_tab_id: Option<TabId>,
    content: Option<String>,
) -> Result<DocumentTabResponse, String> {
    let stores = state.get_stores()?;

    let tab_id = stores
        .document()
        .create_document_tab(
            &doc_id,
            parent_tab_id.as_ref(),
            0, // tab_index - will be ordered by creation time
            &title,
            None, // icon
            content.as_deref(),
            &[], // referenced_assets
            None, // source_tab_id
        )
        .await
        .map_err(|e| e.to_string())?;

    let tab = stores
        .document()
        .get_document_tab(&tab_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Tab not found after creation")?;

    Ok(DocumentTabResponse::from(tab))
}

/// Update a document tab's content
#[tauri::command]
pub async fn update_document_tab_content(
    state: State<'_, Arc<AppState>>,
    tab_id: TabId,
    content: String,
) -> Result<(), String> {
    let stores = state.get_stores()?;

    stores
        .document()
        .update_document_tab_content(&tab_id, &content, &[])
        .await
        .map_err(|e| e.to_string())
}

/// Sync a Google Doc (trigger refresh from MCP server)
/// This will call the MCP server to refresh the document
#[tauri::command]
pub async fn sync_google_doc(
    _state: State<'_, Arc<AppState>>,
    _doc_id: DocumentId,
) -> Result<(), String> {
    // TODO: Call the MCP server to refresh the document
    // The MCP server will:
    // 1. Fetch the latest content from Google Docs
    // 2. Return raw tabs and images
    // 3. noema-core will process and store them
    Err("Sync requires the Google Docs MCP server to be running".to_string())
}

/// Google OAuth configuration status
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct GDocsOAuthStatus {
    /// Whether the server is running
    pub server_running: bool,
    /// The server URL (if running)
    pub server_url: Option<String>,
    /// Whether OAuth credentials are configured
    pub credentials_configured: bool,
    /// Whether user is authenticated (has valid token)
    pub is_authenticated: bool,
}

/// Get the current Google Docs OAuth configuration status
#[tauri::command]
pub async fn get_gdocs_oauth_status() -> Result<GDocsOAuthStatus, String> {
    let config = GoogleOAuthConfig::load().unwrap_or_default();

    Ok(GDocsOAuthStatus {
        server_running: true, // Direct API access is always "running"
        server_url: None, // No MCP server URL
        credentials_configured: config.has_credentials(),
        is_authenticated: config.has_valid_token(),
    })
}

/// Configure Google OAuth credentials
#[tauri::command]
pub async fn configure_gdocs_oauth(
    client_id: String,
    client_secret: Option<String>,
) -> Result<(), String> {
    let mut config = GoogleOAuthConfig::load().unwrap_or_default();
    config.client_id = client_id;
    config.client_secret = client_secret;
    config.save()?;
    Ok(())
}

/// Store OAuth tokens after successful authentication
#[tauri::command]
pub async fn store_gdocs_tokens(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
) -> Result<(), String> {
    let mut config = GoogleOAuthConfig::load().unwrap_or_default();
    config.access_token = Some(access_token);
    config.refresh_token = refresh_token;
    config.expires_at = expires_at;
    config.save()?;
    Ok(())
}

/// Get the Google Docs server URL (for manual MCP connection)
#[tauri::command]
pub async fn get_gdocs_server_url() -> Result<Option<String>, String> {
    // No MCP server - direct API access
    Ok(None)
}

/// Google Doc listing item from Drive
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct GoogleDocListItem {
    pub id: String,
    pub name: String,
    pub modified_time: Option<String>,
    pub created_time: Option<String>,
}

/// List Google Docs from Drive
#[tauri::command]
pub async fn list_google_docs(
    _state: State<'_, Arc<AppState>>,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<GoogleDocListItem>, String> {
    let config = GoogleOAuthConfig::load()
        .ok_or("Google OAuth not configured")?;

    let client = config.create_client()
        .ok_or("Not authenticated with Google. Please sign in first.")?;

    debug!("Calling Google Drive API to list docs, query: {:?}", query);

    let files = client
        .list_documents(query.as_deref(), limit.unwrap_or(20))
        .await
        .map_err(|e| format!("Failed to list Google Docs: {}", e))?;

    Ok(files
        .into_iter()
        .map(|f| GoogleDocListItem {
            id: f.id,
            name: f.name,
            modified_time: f.modified_time,
            created_time: f.created_time,
        })
        .collect())
}

/// Import a Google Doc into local storage
/// Returns the document ID of the imported document
#[tauri::command]
pub async fn import_google_doc(
    state: State<'_, Arc<AppState>>,
    google_doc_id: String,
) -> Result<DocumentInfoResponse, String> {
    info!("Importing Google Doc: {}", google_doc_id);

    // First check if this doc is already imported
    let stores = state.get_stores()?;
    let coordinator = state.get_coordinator()?;
    let document_store = stores.document();

    let user = stores.user()
        .get_or_create_default_user()
        .await
        .map_err(|e| e.to_string())?;

    if let Some(existing) = document_store
        .get_document_by_source(&user.id, DocumentSource::GoogleDrive, &google_doc_id)
        .await
        .map_err(|e| e.to_string())?
    {
        info!("Document already imported, returning existing: {}", existing.id);
        return Ok(DocumentInfoResponse::from(existing));
    }

    // Get Google API client
    let config = GoogleOAuthConfig::load()
        .ok_or("Google OAuth not configured")?;
    let client = config.create_client()
        .ok_or("Not authenticated with Google. Please sign in first.")?;

    debug!("Extracting Google Doc: {}", google_doc_id);

    // Extract document using GoogleDocsClient
    let extracted = client
        .extract_document(&google_doc_id)
        .await
        .map_err(|e| format!("Failed to extract Google Doc: {}", e))?;

    info!(
        "Extracted doc '{}' with {} tabs and {} images",
        extracted.title,
        extracted.tabs.len(),
        extracted.images.len()
    );

    // Create the document
    let doc_id = document_store
        .create_document(
            &user.id,
            &extracted.title,
            DocumentSource::GoogleDrive,
            Some(&google_doc_id),
        )
        .await
        .map_err(|e| format!("Failed to create document: {}", e))?;

    // Store images first so we can reference them in tabs
    let mut image_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for image in &extracted.images {
        // Images come as raw bytes from GoogleDocsClient, encode to base64 for storage
        let data_base64 = base64::engine::general_purpose::STANDARD.encode(&image.data);
        let asset_id = coordinator
            .store_asset(&data_base64, &image.mime_type)
            .await
            .map_err(|e| format!("Failed to store image: {}", e))?;

        image_id_map.insert(image.object_id.clone(), asset_id.into());
    }

    // Build a map of source_tab_id -> internal tab_id for parent references
    let mut tab_id_map: std::collections::HashMap<String, TabId> = std::collections::HashMap::new();

    // Collect referenced asset IDs once
    let referenced_assets: Vec<AssetId> = image_id_map
        .values()
        .map(|hash| AssetId::from_string(hash.clone()))
        .collect();

    // First pass: create all tabs without parent references
    info!("Processing {} tabs with {} image mappings", extracted.tabs.len(), image_id_map.len());
    for tab in &extracted.tabs {
        let mut content = tab.content_markdown.clone();

        // Replace object:OBJECT_ID with noema-asset:// URLs
        for (object_id, blob_hash) in &image_id_map {
            let object_ref = format!("object:{}", object_id);
            let asset_url = format!("noema-asset://localhost/{}", blob_hash);
            if content.contains(&object_ref) {
                info!("Replacing {} -> {} in tab '{}'", object_ref, asset_url, tab.title);
            }
            content = content.replace(&object_ref, &asset_url);
        }

        let source_tab_id = TabId::from_string(tab.source_tab_id.clone());
        let tab_id = document_store
            .create_document_tab(
                &doc_id,
                None, // Set parent in second pass
                tab.tab_index,
                &tab.title,
                tab.icon.as_deref(),
                Some(&content),
                &referenced_assets,
                Some(&source_tab_id),
            )
            .await
            .map_err(|e| format!("Failed to create tab: {}", e))?;

        tab_id_map.insert(tab.source_tab_id.clone(), tab_id);
    }

    // Second pass: update parent references
    for tab in &extracted.tabs {
        if let Some(parent_source_id) = &tab.parent_tab_id {
            if let (Some(tab_id), Some(parent_id)) = (
                tab_id_map.get(&tab.source_tab_id),
                tab_id_map.get(parent_source_id),
            ) {
                document_store
                    .update_document_tab_parent(tab_id, Some(parent_id))
                    .await
                    .map_err(|e| format!("Failed to update tab parent: {}", e))?;
            }
        }
    }

    // Fetch and return the created document
    let doc = document_store
        .get_document(&doc_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Failed to retrieve created document")?;

    info!("Successfully imported Google Doc as: {}", doc_id);
    Ok(DocumentInfoResponse::from(doc))
}

/// Search documents by title for autocomplete
#[tauri::command]
pub async fn search_documents(
    state: State<'_, Arc<AppState>>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<DocumentInfoResponse>, String> {
    let stores = state.get_stores()?;

    let user = stores.user()
        .get_or_create_default_user()
        .await
        .map_err(|e| e.to_string())?;

    let docs = stores.document()
        .search_documents(&user.id, &query, limit.unwrap_or(10))
        .await
        .map_err(|e| e.to_string())?;

    Ok(docs.into_iter().map(DocumentInfoResponse::from).collect())
}

// Note: GoogleDocsClient returns markdown directly from the Google Docs API structured content.
// Each tab has its own content_markdown field with proper per-tab markdown.
