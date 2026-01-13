//! Application initialization command

use config::PathManager;
use noema_core::mcp::{start_auto_connect, ServerStatus};
use noema_core::storage::coordinator::StorageCoordinator;
use noema_core::storage::{FsBlobStore, SqliteStore};
use noema_core::McpRegistry;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::chat::start_engine_event_loop;
use crate::gdocs_server::{self, GDocsServerState};
use crate::logging::log_message;
use crate::state::AppState;

#[tauri::command]
pub async fn init_app(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    // Check and set init flag atomically using std::sync::Mutex
    // We need to drop the guard before any .await points
    let already_initialized = {
        let mut init_guard = state
            .init_lock
            .lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;

        if *init_guard {
            true
        } else {
            // Mark as initializing BEFORE we start - this prevents the race
            *init_guard = true;
            false
        }
    }; // Guard dropped here

    if already_initialized {
        // Don't await anything here - just return empty string
        // The first init will complete and set the real model name
        // The UI will update when it gets the real response
        return Ok(String::new());
    }

    // Run initialization, resetting the lock on error so retry is possible
    match do_init(app, &state).await {
        Ok(result) => Ok(result),
        Err(e) => {
            // Reset the lock so user can retry after fixing the issue
            if let Ok(mut guard) = state.init_lock.lock() {
                *guard = false;
            }
            Err(e)
        }
    }
}

async fn do_init(app: AppHandle, state: &AppState) -> Result<String, String> {
    log_message("Starting app initialization");

    // Load config first so env vars are available
    init_config()?;
    log_message("Config loaded");

    init_storage(state).await.map_err(|e| {
        log_message(&format!("ERROR in init_storage: {}", e));
        e
    })?;
    log_message("Storage initialized");

    init_user(state).await.map_err(|e| {
        log_message(&format!("ERROR in init_user: {}", e));
        e
    })?;
    log_message("User initialized");

    // Initialize default model name (no engine yet - created when conversation is loaded)
    let model_name = init_default_model(state).await?;
    log_message(&format!("Default model set: {}", model_name));

    // Start embedded Google Docs MCP server
    start_gdocs_server(&app).await;

    // Initialize MCP registry (global, not per-conversation)
    let mcp_registry = init_mcp(state)?;
    log_message("MCP registry loaded");

    // Start the engine event loop (runs continuously, polls all loaded engines)
    start_engine_event_loop(app.clone());
    log_message("Event loop started");

    // Start auto-connect for MCP servers (runs in background)
    start_mcp_auto_connect(app, mcp_registry).await;
    log_message("MCP auto-connect started");

    Ok(model_name)
}

/// Start the embedded Google Docs MCP server
async fn start_gdocs_server(app: &AppHandle) {
    let gdocs_state = app.state::<GDocsServerState>();
    match gdocs_server::start_gdocs_server(&gdocs_state).await {
        Ok(url) => {
            log_message(&format!("Google Docs MCP server started at {}", url));
        }
        Err(e) => {
            log_message(&format!("Failed to start Google Docs server: {}", e));
        }
    }
}

/// Start auto-connect for all configured MCP servers
async fn start_mcp_auto_connect(app: AppHandle, mcp_registry: Arc<tokio::sync::Mutex<McpRegistry>>) {

    // Create callback that emits events to frontend
    let app_handle = app.clone();
    let on_status_change: Arc<dyn Fn(&str, &ServerStatus) + Send + Sync> =
        Arc::new(move |server_id: &str, status: &ServerStatus| {
            let status_str = match status {
                ServerStatus::Disconnected => "disconnected".to_string(),
                ServerStatus::Connected => "connected".to_string(),
                ServerStatus::Retrying { attempt } => format!("retrying:{}", attempt),
                ServerStatus::RetryStopped { last_error } => format!("stopped:{}", last_error),
            };

            log_message(&format!("MCP server '{}' status: {}", server_id, status_str));

            // Emit event to frontend
            let _ = app_handle.emit(
                "mcp_server_status",
                serde_json::json!({
                    "server_id": server_id,
                    "status": status_str,
                }),
            );
        });

    let count = start_auto_connect(mcp_registry, Some(on_status_change)).await;
    log_message(&format!("Started auto-connect for {} MCP servers", count));
}

async fn init_storage(state: &AppState) -> Result<(), String> {
    let db_path = PathManager::db_path().ok_or("Failed to determine database path")?;
    let blob_dir = PathManager::blob_storage_dir().ok_or("Failed to determine blob storage path")?;

    if let Some(parent) = db_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create database dir: {}", e))?;
        }
    }

    std::fs::create_dir_all(&blob_dir)
        .map_err(|e| format!("Failed to create blob storage dir: {}", e))?;

    // Create blob store
    let blob_store = Arc::new(FsBlobStore::new(blob_dir));

    // Create the SQL store
    let store = SqliteStore::open(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;
    let store = Arc::new(store);

    // Create storage coordinator with all stores
    // StorageCoordinator<B, A, T, C, U, D>
    // SqliteStore implements: AssetStore, TextStore, ConversationStore, UserStore, DocumentStore
    let coordinator = Arc::new(StorageCoordinator::new(
        blob_store,      // B: BlobStore
        store.clone(),   // A: AssetStore
        store.clone(),   // T: TextStore
        store.clone(),   // C: ConversationStore
        store.clone(),   // U: UserStore
        store.clone(),   // D: DocumentStore
    ));

    let _ = state.coordinator.set(coordinator);
    Ok(())
}

fn init_config() -> Result<(), String> {
    config::load_env_file();
    Ok(())
}

async fn init_user(state: &AppState) -> Result<(), String> {
    let coordinator = state.get_coordinator()?;

    // First check if user email is explicitly configured in settings
    let settings = config::Settings::load();
    let user = if let Some(email) = settings.user_email {
        // User has configured a specific email - get or create that user
        coordinator
            .get_or_create_user_by_email(&email)
            .await
            .map_err(|e| format!("Failed to get/create user: {}", e))?
    } else {
        // No email configured - use smart selection logic
        let users = coordinator
            .list_users()
            .await
            .map_err(|e| format!("Failed to list users: {}", e))?;

        match users.len() {
            0 => {
                // No users exist - create default user
                coordinator
                    .get_or_create_default_user()
                    .await
                    .map_err(|e| format!("Failed to create default user: {}", e))?
            }
            1 => {
                // Exactly one user - use that user
                users.into_iter().next().unwrap()
            }
            _ => {
                // Multiple users - need user to select
                let emails: Vec<String> = users.iter().map(|u| u.email.clone()).collect();
                return Err(format!("MULTIPLE_USERS:{}", emails.join(",")));
            }
        }
    };

    *state.user_id.lock().await = user.id;
    Ok(())
}

/// Initialize MCP registry (global, shared across all engines)
fn init_mcp(state: &AppState) -> Result<Arc<tokio::sync::Mutex<McpRegistry>>, String> {
    let registry = McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));
    let registry_arc = Arc::new(tokio::sync::Mutex::new(registry));

    let _ = state.mcp_registry.set(registry_arc.clone());
    Ok(registry_arc)
}

/// Initialize default model settings (no engine created yet)
async fn init_default_model(state: &AppState) -> Result<String, String> {
    const FALLBACK_MODEL_ID: &str = "claude/models/claude-sonnet-4-5-20250929";

    let settings = config::Settings::load();
    let model_id = settings
        .default_model
        .unwrap_or_else(|| FALLBACK_MODEL_ID.to_string());

    let model_display_name = model_id
        .split('/')
        .last()
        .unwrap_or(&model_id)
        .to_string();

    *state.model_id.lock().await = model_id;
    *state.model_name.lock().await = model_display_name.clone();

    Ok(model_display_name)
}
