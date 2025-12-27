//! Google Docs commands for managing imported documents
//!
//! Uses the episteme-compatible document model with documents, tabs, and revisions.

use noema_core::mcp::{AuthMethod, McpConfig, ServerConfig};
use noema_core::storage::{DocumentInfo, DocumentSource, DocumentTabInfo};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use ts_rs::TS;

use crate::gdocs_server::GDocsServerState;
use crate::state::AppState;

/// Document info response for the frontend
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct DocumentInfoResponse {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub source: String,
    pub source_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<DocumentInfo> for DocumentInfoResponse {
    fn from(info: DocumentInfo) -> Self {
        DocumentInfoResponse {
            id: info.id,
            user_id: info.user_id,
            title: info.title,
            source: info.source.as_str().to_string(),
            source_id: info.source_id,
            created_at: info.created_at,
            updated_at: info.updated_at,
        }
    }
}

/// Document tab response for the frontend
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/generated/")]
pub struct DocumentTabResponse {
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

impl From<DocumentTabInfo> for DocumentTabResponse {
    fn from(tab: DocumentTabInfo) -> Self {
        DocumentTabResponse {
            id: tab.id,
            document_id: tab.document_id,
            parent_tab_id: tab.parent_tab_id,
            tab_index: tab.tab_index,
            title: tab.title,
            icon: tab.icon,
            content_markdown: tab.content_markdown,
            referenced_assets: tab.referenced_assets,
            source_tab_id: tab.source_tab_id,
            current_revision_id: tab.current_revision_id,
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
pub async fn list_documents(state: State<'_, AppState>) -> Result<Vec<DocumentInfoResponse>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    // Get default user
    let user = store
        .get_or_create_default_user()
        .map_err(|e| e.to_string())?;

    let docs = store
        .list_documents(&user.id)
        .map_err(|e| e.to_string())?;
    Ok(docs.into_iter().map(DocumentInfoResponse::from).collect())
}

/// Get a single document by ID
#[tauri::command]
pub async fn get_document(
    state: State<'_, AppState>,
    doc_id: String,
) -> Result<Option<DocumentInfoResponse>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let doc = store.get_document(&doc_id).map_err(|e| e.to_string())?;
    Ok(doc.map(DocumentInfoResponse::from))
}

/// Get document by Google Doc ID (source_id)
#[tauri::command]
pub async fn get_document_by_google_id(
    state: State<'_, AppState>,
    google_doc_id: String,
) -> Result<Option<DocumentInfoResponse>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let user = store
        .get_or_create_default_user()
        .map_err(|e| e.to_string())?;

    let doc = store
        .get_document_by_source(&user.id, DocumentSource::GoogleDrive, &google_doc_id)
        .map_err(|e| e.to_string())?;
    Ok(doc.map(DocumentInfoResponse::from))
}

/// Get document content with all tabs
#[tauri::command]
pub async fn get_document_content(
    state: State<'_, AppState>,
    doc_id: String,
) -> Result<DocumentContentResponse, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    // Get document metadata
    let doc = store
        .get_document(&doc_id)
        .map_err(|e| e.to_string())?
        .ok_or("Document not found")?;

    // Get all tabs for this document
    let tabs = store
        .list_document_tabs(&doc_id)
        .map_err(|e| e.to_string())?;

    Ok(DocumentContentResponse {
        info: DocumentInfoResponse::from(doc),
        tabs: tabs.into_iter().map(DocumentTabResponse::from).collect(),
    })
}

/// Get a single tab's content
#[tauri::command]
pub async fn get_document_tab(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Option<DocumentTabResponse>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let tab = store
        .get_document_tab(&tab_id)
        .map_err(|e| e.to_string())?;
    Ok(tab.map(DocumentTabResponse::from))
}

/// Delete a document and all its tabs/revisions
#[tauri::command]
pub async fn delete_document(state: State<'_, AppState>, doc_id: String) -> Result<bool, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    store.delete_document(&doc_id).map_err(|e| e.to_string())
}

/// Sync a Google Doc (trigger refresh from MCP server)
/// This will call the MCP server to refresh the document
#[tauri::command]
pub async fn sync_google_doc(
    _state: State<'_, AppState>,
    _doc_id: String,
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
pub async fn get_gdocs_oauth_status(
    app: tauri::AppHandle,
) -> Result<GDocsOAuthStatus, String> {
    let gdocs_state = app.state::<GDocsServerState>();
    let server_url = gdocs_state.url().await;
    let server_running = server_url.is_some();

    // Check MCP config for credentials
    let config = McpConfig::load().unwrap_or_default();
    let (credentials_configured, is_authenticated) = if let Some(server) = config.get_server("gdocs") {
        match &server.auth {
            AuthMethod::OAuth { client_id, access_token, .. } => {
                let has_creds = !client_id.is_empty();
                let has_token = access_token.is_some();
                (has_creds, has_token)
            }
            _ => (false, false),
        }
    } else {
        (false, false)
    };

    Ok(GDocsOAuthStatus {
        server_running,
        server_url,
        credentials_configured,
        is_authenticated,
    })
}

/// Configure Google OAuth credentials for the Google Docs MCP server
#[tauri::command]
pub async fn configure_gdocs_oauth(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    client_id: String,
    client_secret: Option<String>,
) -> Result<(), String> {
    let gdocs_state = app.state::<GDocsServerState>();
    let server_url = gdocs_state.url().await
        .ok_or("Google Docs server not running")?;

    // Create the server config with OAuth credentials
    let server_config = ServerConfig {
        name: "Google Docs".to_string(),
        url: server_url,
        auth: AuthMethod::OAuth {
            client_id,
            client_secret,
            authorization_url: Some("https://accounts.google.com/o/oauth2/v2/auth".to_string()),
            token_url: Some("https://oauth2.googleapis.com/token".to_string()),
            scopes: vec![
                "https://www.googleapis.com/auth/drive.readonly".to_string(),
                "https://www.googleapis.com/auth/documents.readonly".to_string(),
            ],
            access_token: None,
            refresh_token: None,
            expires_at: None,
        },
        use_well_known: true,
        auth_token: None,
        auto_connect: true,
        auto_retry: true,
    };

    // Update the in-memory MCP registry
    let engine_guard = state.engine.lock().await;
    if let Some(engine) = engine_guard.as_ref() {
        let mcp_registry = engine.get_mcp_registry();
        let mut registry = mcp_registry.lock().await;
        registry.add_server("gdocs".to_string(), server_config.clone());
        registry.save_config().map_err(|e| format!("Failed to save config: {}", e))?;
    } else {
        // Engine not initialized yet, save directly to config file
        let mut config = McpConfig::load().unwrap_or_default();
        config.add_server("gdocs".to_string(), server_config);
        config.save().map_err(|e| format!("Failed to save config: {}", e))?;
    }

    Ok(())
}

/// Get the Google Docs server URL (for manual MCP connection)
#[tauri::command]
pub async fn get_gdocs_server_url(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let gdocs_state = app.state::<GDocsServerState>();
    Ok(gdocs_state.url().await)
}
