//! Application initialization command

use simply_daemon::api::ModelApi;
use simply_daemon::embedded::EmbeddedDaemon;
use simply_daemon::storage::SqliteStores;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

use crate::logging::log_message;
use crate::state::AppState;

#[tauri::command]
pub async fn init_app(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    if state.is_initialized() {
        return Ok(String::new());
    }

    let state_arc = state.inner().clone();
    do_init(app, state_arc).await
}

async fn do_init(_app: AppHandle, state: Arc<AppState>) -> Result<String, String> {
    log_message("Starting app initialization");

    config::load_env_file();

    let stores = Arc::new(
        SqliteStores::open().map_err(|e| format!("Failed to open storage: {}", e))?
    );

    let daemon = EmbeddedDaemon::new(stores)
        .await
        .map_err(|e| format!("Failed to create daemon: {}", e))?;

    let model_name = daemon.default_model_id().await;
    let _ = state.daemon.set(daemon);

    log_message(&format!("Daemon initialized, default model: {}", model_name));
    Ok(model_name)
}
