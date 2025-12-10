//! Application initialization command

use config::PathManager;
use llm::create_model;
use noema_core::storage::BlobStore;
use noema_core::{ChatEngine, McpRegistry, SqliteSession, SqliteStore};
use std::sync::Arc;
use tauri::{AppHandle, State};

use crate::commands::chat::start_engine_event_loop;
use crate::state::AppState;

#[tauri::command]
pub async fn init_app(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
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
    // Load config first so env vars are available
    init_config()?;
    init_storage(state).await.map_err(|e| {
        eprintln!("ERROR in init_storage: {}", e);
        e
    })?;
    init_user(state).await.map_err(|e| {
        eprintln!("ERROR in init_user: {}", e);
        e
    })?;
    let session = init_session(state).await.map_err(|e| {
        eprintln!("ERROR in init_session: {}", e);
        e
    })?;
    let mcp_registry = init_mcp()?;
    let result = init_engine(state, session, mcp_registry).await?;

    // Start the engine event loop (runs continuously)
    start_engine_event_loop(app);

    Ok(result)
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

    let store = SqliteStore::open(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;

    let blob_store = Arc::new(BlobStore::new(blob_dir));

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
        // User has configured a specific email - use that
        store
            .get_user_by_email(&email)
            .map_err(|e| format!("Failed to query user: {}", e))?
            .ok_or_else(|| format!("User not found: {}. Create the user in Episteme first.", email))?
    } else {
        // No email configured - use smart selection logic
        let users = store
            .list_users()
            .map_err(|e| format!("Failed to list users: {}", e))?;

        match users.len() {
            0 => {
                // No users exist - create default user
                store
                    .get_or_create_default_user()
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
    *state.user_id.lock().await = user.id;
    Ok(())
}

async fn init_session(state: &AppState) -> Result<SqliteSession, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;
    let user_id = state.user_id.lock().await.clone();

    // Try to open the most recent conversation, or create a new one if none exist
    let conversations = store
        .list_conversations(&user_id)
        .map_err(|e| format!("Failed to list conversations: {}", e))?;

    let session = if let Some(most_recent) = conversations.first() {
        // Get blob store for resolver
        let blob_store_guard = state.blob_store.lock().await;
        let blob_store = blob_store_guard
            .as_ref()
            .ok_or("Blob store not initialized")?
            .clone();
        drop(blob_store_guard);

        // Open the most recent conversation with blob resolver
        store
            .open_conversation(&most_recent.id, move |asset_id: String| {
                let blob_store = blob_store.clone();
                async move {
                    blob_store.get(&asset_id)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                }
            })
            .await
            .map_err(|e| format!("Failed to open conversation: {}", e))?
    } else {
        // No conversations exist, create a new one
        store
            .create_conversation()
            .map_err(|e| format!("Failed to create conversation: {}", e))?
    };

    let conversation_id = session.conversation_id().to_string();
    drop(store_guard); // Release lock before acquiring another
    *state.current_conversation_id.lock().await = conversation_id;

    Ok(session)
}

fn init_mcp() -> Result<McpRegistry, String> {
    Ok(McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default())))
}

async fn init_engine(
    state: &AppState,
    session: SqliteSession,
    mcp_registry: McpRegistry,
) -> Result<String, String> {
    let default_model_id = "gemini/models/gemini-2.5-flash";
    let model =
        create_model(default_model_id).map_err(|e| format!("Failed to create model: {}", e))?;

    let model_display_name = default_model_id
        .split('/')
        .last()
        .unwrap_or(default_model_id);

    *state.model_id.lock().await = default_model_id.to_string();
    *state.model_name.lock().await = model_display_name.to_string();

    let engine = ChatEngine::new(session, model, mcp_registry);
    *state.engine.lock().await = Some(engine);

    Ok(model_display_name.to_string())
}
