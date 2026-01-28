//! Google Docs commands for managing imported documents
//!
//! Uses the episteme-compatible document model with documents, tabs, and revisions.

use noema_core::mcp::{AuthMethod, McpConfig, ServerConfig};
use noema_core::storage::ids::{AssetId, DocumentId, RevisionId, TabId, UserId};
use noema_core::storage::{Document, DocumentSource, DocumentStore, DocumentTab, StoredEditable, Stores, UserStore};
use rmcp::model::RawContent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, State};
use tracing::{debug, info};
use ts_rs::TS;

use crate::gdocs_server::GDocsServerState;
use crate::state::AppState;

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
    state: State<'_, Arc<AppState>>,
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

    // Update the in-memory MCP registry if available, otherwise save to config file
    if let Ok(mcp_registry) = state.get_mcp_registry() {
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

/// List Google Docs from Drive via MCP server
#[tauri::command]
pub async fn list_google_docs(
    state: State<'_, Arc<AppState>>,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<GoogleDocListItem>, String> {
    // Get a cloneable tool caller so we don't hold the lock during the async call
    let tool_caller = {
        let mcp_registry = state.get_mcp_registry()?;
        let registry = mcp_registry.lock().await;

        // Find the gdocs server connection and get a cloneable tool caller
        registry
            .get_connection("gdocs")
            .ok_or("Google Docs server not connected. Please authenticate first.")?
            .tool_caller()
        // Locks are dropped here when the block ends
    };

    // Build arguments for gdocs_list tool
    let mut args = serde_json::Map::new();
    if let Some(q) = query {
        args.insert("query".to_string(), serde_json::Value::String(q));
    }
    args.insert(
        "limit".to_string(),
        serde_json::Value::Number(serde_json::Number::from(limit.unwrap_or(20))),
    );

    debug!("Calling gdocs_list with args: {:?}", args);

    // Call the MCP server (without holding any locks)
    let result = tool_caller
        .call_tool("gdocs_list".to_string(), Some(args))
        .await
        .map_err(|e| format!("Failed to list Google Docs: {}", e))?;

    // Check if the result is an error
    if result.is_error.unwrap_or(false) {
        let error_text = result
            .content
            .first()
            .and_then(|c| match &c.raw {
                RawContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("Unknown error");
        return Err(error_text.to_string());
    }

    // Parse the result - it's a JSON array of documents
    let content = result
        .content
        .first()
        .ok_or("Empty response from gdocs_list")?;

    let text = match &content.raw {
        RawContent::Text(text_content) => &text_content.text,
        _ => return Err("Invalid response format: expected text".to_string()),
    };

    let docs: Vec<GoogleDocListItem> =
        serde_json::from_str(text).map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(docs)
}

/// Response from gdocs_extract MCP tool
#[derive(Debug, Deserialize)]
struct ExtractResponse {
    doc_id: String,
    title: String,
    tabs: Vec<ExtractedTab>,
    images: Vec<ExtractedImage>,
}

#[derive(Debug, Deserialize)]
struct ExtractedTab {
    source_tab_id: String,
    title: String,
    icon: Option<String>,
    /// Markdown content for this tab (converted by MCP server from Docs API)
    content_markdown: String,
    parent_tab_id: Option<String>,
    tab_index: i32,
}

#[derive(Debug, Deserialize)]
struct ExtractedImage {
    object_id: String,
    data_base64: String,
    mime_type: String,
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

    // Get a cloneable tool caller so we don't hold the lock during the async call
    let tool_caller = {
        let mcp_registry = state.get_mcp_registry()?;
        let registry = mcp_registry.lock().await;

        registry
            .get_connection("gdocs")
            .ok_or("Google Docs server not connected. Please authenticate first.")?
            .tool_caller()
        // Locks are dropped here when the block ends
    };

    // Call the MCP server to extract the document (without holding any locks)
    let mut args = serde_json::Map::new();
    args.insert(
        "doc_id".to_string(),
        serde_json::Value::String(google_doc_id.clone()),
    );

    debug!("Calling gdocs_extract for doc: {}", google_doc_id);

    let result = tool_caller
        .call_tool("gdocs_extract".to_string(), Some(args))
        .await
        .map_err(|e| format!("Failed to extract Google Doc: {}", e))?;

    // Check if the result is an error
    if result.is_error.unwrap_or(false) {
        let error_text = result
            .content
            .first()
            .and_then(|c| match &c.raw {
                RawContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("Unknown error");
        return Err(error_text.to_string());
    }

    let content = result
        .content
        .first()
        .ok_or("Empty response from gdocs_extract")?;

    let text = match &content.raw {
        RawContent::Text(text_content) => &text_content.text,
        _ => return Err("Invalid response format: expected text".to_string()),
    };

    let extract_response: ExtractResponse =
        serde_json::from_str(text).map_err(|e| format!("Failed to parse extract response: {}", e))?;

    info!(
        "Extracted doc '{}' with {} tabs and {} images",
        extract_response.title,
        extract_response.tabs.len(),
        extract_response.images.len()
    );

    // Store the document and its content
    // (We already have the stores and user from the check above)

    // Create the document
    let doc_id = document_store
        .create_document(
            &user.id,
            &extract_response.title,
            DocumentSource::GoogleDrive,
            Some(&google_doc_id),
        )
        .await
        .map_err(|e| format!("Failed to create document: {}", e))?;

    // Store images first so we can reference them in tabs
    let mut image_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for image in &extract_response.images {
        let asset_id = coordinator
            .store_asset(&image.data_base64, &image.mime_type)
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
    // Each tab now has its own markdown content from the Docs API structured content
    // Replace object:OBJECT_ID with asset:BLOB_HASH so frontend can resolve to URLs
    info!("Processing {} tabs with {} image mappings", extract_response.tabs.len(), image_id_map.len());
    for tab in &extract_response.tabs {
        let mut content = tab.content_markdown.clone();

        // Debug: log first 500 chars of content to see list formatting
        if content.contains("- ") {
            debug!("Tab '{}' markdown sample (first 500 chars):\n{}", tab.title, content.chars().take(500).collect::<String>());
        }

        // Log if this tab has any image references
        if content.contains("object:") {
            info!("Tab '{}' contains object: references", tab.title);
        }
        if content.contains("![image]") {
            info!("Tab '{}' contains ![image] markdown, first 200 chars: {}", tab.title, &content.chars().take(200).collect::<String>());
        }

        for (object_id, blob_hash) in &image_id_map {
            let object_ref = format!("object:{}", object_id);
            // Use full noema-asset:// URL so frontend doesn't need to rewrite
            let asset_url = format!("noema-asset://localhost/{}", blob_hash);
            if content.contains(&object_ref) {
                info!("Replacing {} -> {} in tab '{}'", object_ref, asset_url, tab.title);
            }
            content = content.replace(&object_ref, &asset_url);
        }

        // Log final state
        if content.contains("noema-asset://") {
            info!("Tab '{}' now contains noema-asset:// URLs", tab.title);
        } else if content.contains("![image]") {
            info!("Tab '{}' still has ![image] but no noema-asset:// URLs - content sample: {}", tab.title, &content.chars().take(300).collect::<String>());
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
    for tab in &extract_response.tabs {
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

// HTML to Markdown conversion is no longer needed!
// The MCP server now returns markdown directly from the Google Docs API structured content.
// Each tab has its own content_markdown field with proper per-tab markdown.
