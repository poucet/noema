//! Tauri bridge for Noema - connects React frontend to noema-core

mod commands;
mod logging;
mod state;
mod types;

use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

pub use logging::log_message;
pub use state::AppState;
pub use types::*;

// Re-export commands module
pub use commands::*;

// ============================================================================
// Application Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
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
        .setup(|app| {
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
            commands::chat::init_app,
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
            // File commands
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}