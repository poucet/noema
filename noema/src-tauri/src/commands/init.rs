//! Application initialization command

use simply_daemon::api::ModelApi;
use simply_daemon::ws;
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
    let settings = config::Settings::load();
    let port = settings.daemon_port;

    let handle = ws::connect_or_host(port)
        .await
        .map_err(|e| format!("Failed to initialize daemon: {}", e))?;

    let is_host = handle.is_host();
    let daemon = handle.daemon();

    let model_name = daemon.default_model_id().await;
    let _ = state.daemon.set(daemon);
    let _ = state._daemon_handle.set(handle);

    log_message(&format!(
        "Daemon initialized ({}), default model: {}",
        if is_host { "embedded" } else { "remote" },
        model_name
    ));
    Ok(model_name)
}
