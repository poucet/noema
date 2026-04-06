//! Application initialization command

use simply_daemon::api::*;
use simply_daemon::net;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

use crate::logging::log_message;
use crate::state::AppState;

#[tauri::command]
pub async fn init_app(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    if state.is_initialized() {
        return Ok(String::new());
    }

    // Prevent concurrent init (React StrictMode calls this twice)
    if state.initializing.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(String::new());
    }

    let state_arc = state.inner().clone();
    do_init(app, state_arc).await
}

async fn do_init(_app: AppHandle, state: Arc<AppState>) -> Result<String, String> {
    log_message("Starting app initialization");

    config::load_env_file();
    let mut settings = config::Settings::load();
    let port = settings.daemon_port;
    let daemon_secret = settings.ensure_daemon_secret().to_string();

    let daemon_port = port.unwrap_or(config::DEFAULT_DAEMON_PORT);

    let handle = net::connect_or_host(port, "noema")
        .await
        .map_err(|e| format!("Failed to initialize daemon: {}", e))?;

    let is_host = handle.is_host();
    let daemon = handle.daemon();

    // Set the REST base URL — used by asset protocol handler and other REST clients.
    // When remote, this points to wherever the daemon is running.
    let rest_base_url: String = format!("http://127.0.0.1:{daemon_port}");
    let _ = state.rest_base_url.set(rest_base_url);

    // Build an authenticated HTTP client for REST proxying (e.g. asset protocol)
    let mut headers = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {daemon_secret}")) {
        headers.insert(reqwest::header::AUTHORIZATION, v);
    }
    let http_client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let _ = state.http_client.set(http_client);

    let model_name = daemon.model().default_model_id().await;
    let _ = state.daemon.set(daemon);
    let _ = state._daemon_handle.set(handle);

    log_message(&format!(
        "Daemon initialized ({}), default model: {}",
        if is_host { "embedded" } else { "remote" },
        model_name
    ));
    Ok(model_name)
}
