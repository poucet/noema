//! Application initialization command

use config::PathManager;
use llm::create_model;
use noema_core::mcp::{start_auto_connect, ServerStatus};
use noema_core::storage::{
    AssetStore, BlobStore, ContentBlockStore, ConversationStore, DocumentResolver, FsBlobStore,
    Session, SqliteStore, UserStore,
};
use noema_core::storage::coordinator::DynStorageCoordinator;
use noema_core::storage::ids::ConversationId;
use noema_core::{ChatEngine, McpRegistry};
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

    let (session, store) = init_session(state).await.map_err(|e| {
        log_message(&format!("ERROR in init_session: {}", e));
        e
    })?;
    log_message("Session initialized");

    // Start embedded Google Docs MCP server
    start_gdocs_server(&app).await;

    let mcp_registry = init_mcp()?;
    log_message("MCP registry loaded");

    let result = init_engine(state, session, store, mcp_registry).await?;
    log_message(&format!("Engine initialized with model: {}", result));

    // Start the engine event loop (runs continuously)
    start_engine_event_loop(app.clone());
    log_message("Event loop started");

    // Start auto-connect for MCP servers (runs in background)
    start_mcp_auto_connect(app, state).await;
    log_message("MCP auto-connect started");

    Ok(result)
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
async fn start_mcp_auto_connect(app: AppHandle, state: &AppState) {
    // Get the MCP registry from the engine
    let engine_guard = state.engine.lock().await;
    let engine = match engine_guard.as_ref() {
        Some(e) => e,
        None => {
            log_message("Cannot start MCP auto-connect: engine not initialized");
            return;
        }
    };

    let mcp_registry = engine.get_mcp_registry();
    drop(engine_guard);

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

    // Create blob store first (needed for coordinator)
    let blob_store = Arc::new(FsBlobStore::new(blob_dir)) as Arc<dyn BlobStore>;

    // Create the SQL store
    let store = SqliteStore::open(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;

    // Wrap store in Arc so we can share it
    let store = Arc::new(store);

    // Create storage coordinator for automatic asset externalization
    // The coordinator uses the blob store for binary data and the SQL store for asset metadata
    // Note: SqliteStore implements AssetStore and ContentBlockStore
    let asset_store = Arc::clone(&store) as Arc<dyn AssetStore>;
    let content_block_store = Arc::clone(&store) as Arc<dyn ContentBlockStore>;
    let coordinator = Arc::new(DynStorageCoordinator::new(
        blob_store.clone(),
        asset_store,
        content_block_store,
    ));

    // Set the coordinator on the store (uses interior mutability)
    store.set_coordinator(coordinator);

    *state.store.lock().await = Some(store);
    *state.blob_store.lock().await = Some(blob_store);
    Ok(())
}

fn init_config() -> Result<(), String> {
    config::load_env_file();
    Ok(())
}

async fn init_user(state: &AppState) -> Result<(), String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    // First check if user email is explicitly configured in settings
    let settings = config::Settings::load();
    let user = if let Some(email) = settings.user_email {
        // User has configured a specific email - get or create that user
        store
            .get_or_create_user_by_email(&email)
            .await
            .map_err(|e| format!("Failed to get/create user: {}", e))?
    } else {
        // No email configured - use smart selection logic
        let users = store
            .list_users()
            .await
            .map_err(|e| format!("Failed to list users: {}", e))?;

        match users.len() {
            0 => {
                // No users exist - create default user
                store
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

    drop(store_guard);
    *state.user_id.lock().await = noema_core::storage::ids::UserId::from_string(user.id);
    Ok(())
}

async fn init_session(state: &AppState) -> Result<(Session<SqliteStore, SqliteStore>, Arc<SqliteStore>), String> {
    let store = {
        let store_guard = state.store.lock().await;
        store_guard
            .as_ref()
            .ok_or("Storage not initialized")?
            .clone()
    };
    let user_id = state.user_id.lock().await.clone();

    // Try to open the most recent conversation, or create a new one if none exist
    let conversations = store
        .list_conversations(&user_id)
        .await
        .map_err(|e| format!("Failed to list conversations: {}", e))?;

    let conversation_id = if let Some(most_recent) = conversations.first() {
        most_recent.id.clone()
    } else {
        // No conversations exist, create a new one
        store
            .create_conversation(&user_id, None)
            .await
            .map_err(|e| format!("Failed to create conversation: {}", e))?
    };

    // Open session for the conversation
    // Session::open takes TurnStore and ContentBlockStore - SqliteStore implements both
    let session = Session::open(Arc::clone(&store), Arc::clone(&store), conversation_id.clone())
        .await
        .map_err(|e| format!("Failed to open session: {}", e))?;

    *state.current_conversation_id.lock().await = conversation_id.as_str().to_string();

    Ok((session, store))
}

fn init_mcp() -> Result<McpRegistry, String> {
    Ok(McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default())))
}

async fn init_engine(
    state: &AppState,
    session: Session<SqliteStore, SqliteStore>,
    store: Arc<SqliteStore>,
    mcp_registry: McpRegistry,
) -> Result<String, String> {
    const FALLBACK_MODEL_ID: &str = "claude/models/claude-sonnet-4-5-20250929";

    // Load default model from settings, fall back to hardcoded default
    let settings = config::Settings::load();
    let model_id = settings
        .default_model
        .unwrap_or_else(|| FALLBACK_MODEL_ID.to_string());

    let model =
        create_model(&model_id).map_err(|e| format!("Failed to create model: {}", e))?;

    let model_display_name = model_id
        .split('/')
        .last()
        .unwrap_or(&model_id)
        .to_string();

    *state.model_id.lock().await = model_id;
    *state.model_name.lock().await = model_display_name.clone();

    // Store implements DocumentResolver directly
    let document_resolver: Arc<dyn DocumentResolver> = store;

    let engine = ChatEngine::new(session, model, mcp_registry, document_resolver);
    *state.engine.lock().await = Some(engine);

    Ok(model_display_name)
}
