//! Tauri bridge for Noema - connects React frontend to noema-core

mod commands;
mod gdocs_server;
mod logging;
mod oauth_callback;
mod state;
mod types;

use config::PathManager;
use noema_core::storage::BlobStore;
use tauri::http::Response;
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

pub use logging::log_message;
pub use state::AppState;
pub use types::*;

// Re-export commands module
pub use commands::*;

// ============================================================================
// Asset Protocol Handler
// ============================================================================

/// Handle requests to noema-asset://localhost/{asset_id}
/// Returns the asset with proper caching headers
fn handle_asset_request(request: &tauri::http::Request<Vec<u8>>) -> Response<Vec<u8>> {
    // Parse asset_id from path: /asset_id or /{asset_id}
    let path = request.uri().path();
    let asset_id = path.trim_start_matches('/');

    if asset_id.is_empty() {
        return Response::builder()
            .status(400)
            .header("Content-Type", "text/plain")
            .body("Missing asset_id".as_bytes().to_vec())
            .unwrap();
    }

    // Get blob storage directory
    let blob_dir = match PathManager::blob_storage_dir() {
        Some(dir) => dir,
        None => {
            return Response::builder()
                .status(500)
                .header("Content-Type", "text/plain")
                .body("Blob storage not configured".as_bytes().to_vec())
                .unwrap();
        }
    };

    let blob_store = BlobStore::new(blob_dir);

    // Read the asset
    let data = match blob_store.get(asset_id) {
        Ok(data) => data,
        Err(_) => {
            return Response::builder()
                .status(404)
                .header("Content-Type", "text/plain")
                .body(format!("Asset not found: {}", asset_id).into_bytes())
                .unwrap();
        }
    };

    // Determine content type from asset_id query param or default
    // Frontend should pass ?mime_type=image/png in the URL
    let mime_type = request
        .uri()
        .query()
        .and_then(|q| {
            q.split('&')
                .find(|p| p.starts_with("mime_type="))
                .map(|p| p.trim_start_matches("mime_type="))
        })
        .unwrap_or("application/octet-stream");

    // Build response with caching headers
    // Assets are immutable (content-addressed), so we can cache forever
    Response::builder()
        .status(200)
        .header("Content-Type", mime_type)
        .header("Content-Length", data.len().to_string())
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .header("ETag", format!("\"{}\"", asset_id))
        .body(data)
        .unwrap()
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // Custom protocol for serving assets from blob storage
        // Assets are served at: noema-asset://localhost/{asset_id}
        // Browser can cache these using standard HTTP caching
        .register_asynchronous_uri_scheme_protocol("noema-asset", |_ctx, request, responder| {
            std::thread::spawn(move || {
                let response = handle_asset_request(&request);
                responder.respond(response);
            });
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
        .manage(AppState::new())
        .manage(gdocs_server::GDocsServerState::default())
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
            commands::chat::send_message_with_attachments,
            commands::chat::clear_history,
            commands::chat::set_model,
            commands::chat::list_models,
            commands::chat::list_conversations,
            commands::chat::switch_conversation,
            commands::chat::new_conversation,
            commands::chat::delete_conversation,
            commands::chat::rename_conversation,
            commands::chat::get_model_name,
            commands::chat::get_current_conversation_id,
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
            commands::files::store_asset,
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
            // Document commands (episteme-compatible)
            commands::gdocs::list_documents,
            commands::gdocs::get_document,
            commands::gdocs::get_document_by_google_id,
            commands::gdocs::get_document_content,
            commands::gdocs::get_document_tab,
            commands::gdocs::delete_document,
            commands::gdocs::sync_google_doc,
            // Google Docs OAuth commands
            commands::gdocs::get_gdocs_oauth_status,
            commands::gdocs::configure_gdocs_oauth,
            commands::gdocs::get_gdocs_server_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}