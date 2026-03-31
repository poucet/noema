//! Application initialization command

use config::PathManager;
use simply_daemon::embedded::EmbeddedDaemon;
use simply_daemon::types::{
    FsBlobStore, McpRegistry, SqliteStore,
    StorageCoordinator, Stores, UserStore,
};
use crate::state::{AppState, AppStorage, AppStores};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

use crate::logging::log_message;

#[tauri::command]
pub async fn init_app(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let already_initialized = {
        let mut init_guard = state
            .init_lock
            .lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;

        if *init_guard {
            true
        } else {
            *init_guard = true;
            false
        }
    };

    if already_initialized {
        return Ok(String::new());
    }

    let state_arc = state.inner().clone();
    match do_init(app, state_arc).await {
        Ok(result) => Ok(result),
        Err(e) => {
            if let Ok(mut guard) = state.init_lock.lock() {
                *guard = false;
            }
            Err(e)
        }
    }
}

async fn do_init(app: AppHandle, state: Arc<AppState>) -> Result<String, String> {
    log_message("Starting app initialization");

    init_config()?;
    log_message("Config loaded");

    init_storage(&state).await.map_err(|e| {
        log_message(&format!("ERROR in init_storage: {}", e));
        e
    })?;
    log_message("Storage initialized");

    init_user(&state).await.map_err(|e| {
        log_message(&format!("ERROR in init_user: {}", e));
        e
    })?;
    log_message("User initialized");

    let model_name = init_daemon(&state).await?;
    log_message(&format!("Daemon initialized with model: {}", model_name));

    Ok(model_name)
}

/// Initialize the daemon — creates EmbeddedDaemon with storage and MCP registry
async fn init_daemon(state: &AppState) -> Result<String, String> {
    use simply_daemon::api::ModelApi;

    let coordinator = state.get_coordinator()?;
    let stores = state.get_stores()?;
    let user_id = state.user_id.lock().await.clone();

    // Initialize MCP registry
    let registry = McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));
    let registry_arc = Arc::new(tokio::sync::Mutex::new(registry));
    let _ = state.mcp_registry.set(registry_arc.clone());

    let stores_arc = Arc::new(AppStores::new(stores.turn(), stores.blob()));

    let daemon = EmbeddedDaemon::new(
        coordinator,
        stores_arc,
        registry_arc,
        user_id,
    )
    .await
    .map_err(|e| format!("Failed to create daemon: {}", e))?;

    let model_id = daemon.default_model_id().await;
    let model_display_name = model_id
        .split('/')
        .last()
        .unwrap_or(&model_id)
        .to_string();

    let _ = state.daemon.set(daemon);

    Ok(model_display_name)
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

    let blob_store = Arc::new(FsBlobStore::new(blob_dir));
    let sqlite_store = SqliteStore::open(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;
    let sqlite_store = Arc::new(sqlite_store);

    let stores = AppStores::new(sqlite_store, blob_store);
    let coordinator = Arc::new(StorageCoordinator::<AppStorage>::from_stores(&stores));
    let _ = state.coordinator.set(coordinator);

    state.init_stores(stores)?;
    Ok(())
}

fn init_config() -> Result<(), String> {
    config::load_env_file();
    Ok(())
}

async fn init_user(state: &AppState) -> Result<(), String> {
    let stores = state.get_stores()?;
    let user_store = stores.user();

    let settings = config::Settings::load();
    let user = if let Some(email) = settings.user_email {
        user_store
            .get_or_create_user_by_email(&email)
            .await
            .map_err(|e| format!("Failed to get/create user: {}", e))?
    } else {
        let users = user_store
            .list_users()
            .await
            .map_err(|e| format!("Failed to list users: {}", e))?;

        match users.len() {
            0 => {
                user_store
                    .get_or_create_default_user()
                    .await
                    .map_err(|e| format!("Failed to create default user: {}", e))?
            }
            1 => {
                users.into_iter().next().unwrap()
            }
            _ => {
                let emails: Vec<String> = users.iter().map(|u| u.email.clone()).collect();
                return Err(format!("MULTIPLE_USERS:{}", emails.join(",")));
            }
        }
    };

    *state.user_id.lock().await = user.id;
    Ok(())
}
