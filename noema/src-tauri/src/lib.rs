//! Tauri bridge for Noema - connects React frontend to noema-core

mod commands;

mod logging;
mod state;
mod types;

use config::PathManager;
use tauri::http::Response;
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;
use std::sync::Arc;

pub use logging::{init_logging, log_message};
pub use state::AppState;
pub use types::*;

// Re-export commands module
pub use commands::*;

// ============================================================================
// Asset Protocol Handler
// ============================================================================

/// Handle requests to noema-asset://localhost/{blob_hash}?mime_type=X
///
/// Proxies to the daemon's REST endpoint at http://localhost:{rest_port}/asset/{hash}.
/// The daemon handles caching headers. If the daemon isn't available (e.g. still
/// initializing), falls back to a direct call.
async fn handle_asset_request(
    request: &tauri::http::Request<Vec<u8>>,
    app_state: Arc<AppState>,
) -> Response<Vec<u8>> {
    let path = request.uri().path();
    let blob_hash = path.trim_start_matches('/');

    if blob_hash.is_empty() {
        return Response::builder()
            .status(400)
            .header("Content-Type", "text/plain")
            .body("Missing blob_hash".as_bytes().to_vec())
            .unwrap();
    }

    let query = request.uri().query().unwrap_or("");

    // Proxy to daemon REST endpoint
    let base_url = match app_state.rest_base_url.get() {
        Some(url) => url.as_str(),
        None => {
            return Response::builder()
                .status(503)
                .header("Content-Type", "text/plain")
                .body("Daemon not initialized".as_bytes().to_vec())
                .unwrap();
        }
    };
    let url = format!("{base_url}/asset/{blob_hash}?{query}");

    match reqwest::get(&url).await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let mut builder = Response::builder().status(status);
            // Forward cache headers from daemon
            for (key, value) in resp.headers() {
                if let Ok(v) = value.to_str() {
                    builder = builder.header(key.as_str(), v);
                }
            }
            let body = resp.bytes().await.unwrap_or_default().to_vec();
            builder.body(body).unwrap()
        }
        Err(e) => {
            Response::builder()
                .status(502)
                .header("Content-Type", "text/plain")
                .body(format!("Daemon REST unavailable: {e}").into_bytes())
                .unwrap()
        }
    }
}

// ============================================================================
// Application Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Check for NOEMA_LOG_FILE environment variable
    if let Ok(path) = std::env::var("NOEMA_LOG_FILE") {
        PathManager::set_log_file(std::path::PathBuf::from(path));
    }

    // Initialize unified tracing/logging - writes to ~/.local/share/noema/logs/noema.log
    init_logging();

    let app_state = Arc::new(AppState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // Custom protocol for serving assets from blob storage
        // Assets are served at: noema-asset://localhost/{asset_id}
        // Browser can cache these using standard HTTP caching
        .register_asynchronous_uri_scheme_protocol("noema-asset", {
            let app_state = app_state.clone();
            move |_ctx, request, responder| {
                let app_state = app_state.clone();
                tauri::async_runtime::spawn(async move {
                    let response = handle_asset_request(&request, app_state).await;
                    responder.respond(response);
                });
            }
        })
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // When a second instance is launched, this callback receives the args
            // Check if any arg is a deep link URL
            log_message(&format!("Single instance callback, argv: {:?}", argv));
            for arg in argv {
                if arg.starts_with("noema://") {
                    if let Ok(url) = url::Url::parse(&arg) {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            commands::mcp::handle_deep_link(&handle, vec![url]).await;
                        });
                    }
                }
            }
            // Focus the existing window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .manage(app_state.clone())
        .setup(|app| {
            #[cfg(any(target_os = "android", target_os = "ios"))]
            {
                use tauri::Manager;
                if let Ok(dir) = app.path().app_data_dir() {
                    config::PathManager::set_data_dir(dir);
                }
            }

            // Register deep link handler for when app is already running
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                let urls = event.urls();
                log_message(&format!("Deep link on_open_url, urls: {:?}", urls));
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    commands::mcp::handle_deep_link(&handle, urls).await;
                });
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Chat commands
            commands::init::init_app,
            commands::chat::get_messages,
            commands::chat::send_message,
            commands::chat::clear_history,
            commands::chat::set_model,
            commands::chat::list_models,
            commands::chat::list_conversations,
            commands::chat::load_conversation,
            commands::chat::new_conversation,
            commands::chat::delete_conversation,
            commands::chat::rename_conversation,
            commands::chat::get_model_name,
            commands::chat::get_favorite_models,
            commands::chat::toggle_favorite_model,
            // Voice commands
            commands::voice::is_voice_available,
            commands::voice::download_voice_model,
            commands::voice::toggle_voice,
            commands::voice::get_voice_status,
            commands::voice::start_voice_session,
            commands::voice::process_audio_chunk,
            commands::voice::stop_voice_session,
            // File/Asset commands
            commands::files::save_file,
            // Logging
            logging::log_debug,
            // MCP server commands
            commands::mcp::list_mcp_servers,
            commands::mcp::add_mcp_server,
            commands::mcp::remove_mcp_server,
            commands::mcp::connect_mcp_server,
            commands::mcp::disconnect_mcp_server,
            commands::mcp::get_mcp_server_tools,
            commands::mcp::test_mcp_server,
            commands::mcp::start_mcp_oauth,
            commands::mcp::complete_mcp_oauth,
            commands::mcp::update_mcp_server_settings,
            commands::mcp::stop_mcp_retry,
            commands::mcp::start_mcp_retry,
            // Settings commands
            commands::settings::get_user_email,
            commands::settings::set_user_email,
            commands::settings::get_api_key_status,
            commands::settings::set_api_key,
            commands::settings::remove_api_key,
            commands::settings::get_provider_info,
            // Document commands — disabled until DocumentApi / sidecar design is resolved
            // commands::gdocs::list_documents,
            // commands::gdocs::get_document,
            // commands::gdocs::get_document_by_google_id,
            // commands::gdocs::get_document_content,
            // commands::gdocs::get_document_tab,
            // commands::gdocs::delete_document,
            // commands::gdocs::create_document,
            // commands::gdocs::update_document_title,
            // commands::gdocs::create_document_tab,
            // commands::gdocs::update_document_tab_content,
            // commands::gdocs::sync_google_doc,
            // commands::gdocs::get_gdocs_oauth_status,
            // commands::gdocs::configure_gdocs_oauth,
            // commands::gdocs::store_gdocs_tokens,
            // commands::gdocs::get_gdocs_server_url,
            // commands::gdocs::list_google_docs,
            // commands::gdocs::import_google_doc,
            // commands::gdocs::search_documents,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}