//! Tauri shell for Noema.
//!
//! The UI is the shared admin bundle from `simply-daemon/admin`. Tauri's job
//! shrinks to: bootstrap the daemon (embedded or remote), expose its HTTP base
//! URL to the webview, and forward OS deep links (OAuth callbacks) into the
//! daemon.

mod commands;
mod logging;
mod state;
mod types;

use config::PathManager;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

pub use logging::{init_logging, log_message};
pub use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Ok(path) = std::env::var("NOEMA_LOG_FILE") {
        PathManager::set_log_file(std::path::PathBuf::from(path));
    }
    init_logging();

    let app_state = Arc::new(AppState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            log_message(&format!("Single instance callback, argv: {:?}", argv));
            for arg in argv {
                if arg.starts_with("noema://") {
                    if let Ok(url) = url::Url::parse(&arg) {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            commands::init::handle_deep_link(&handle, vec![url]).await;
                        });
                    }
                }
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .manage(app_state.clone())
        .setup({
            let app_state = app_state.clone();
            move |app| {
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    use tauri::Manager;
                    if let Ok(dir) = app.path().app_data_dir() {
                        config::PathManager::set_data_dir(dir);
                    }
                }

                // Boot the daemon (embedded or remote) before the webview
                // asks for `daemon_base_url`. The admin UI has no init step,
                // so we can't wait for a frontend call to kick this off.
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = commands::init::run_init(app_state).await {
                        log_message(&format!("Daemon init failed: {e}"));
                    }
                });

                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    let urls = event.urls();
                    log_message(&format!("Deep link on_open_url, urls: {:?}", urls));
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        commands::init::handle_deep_link(&handle, urls).await;
                    });
                });
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::init::init_app,
            commands::init::daemon_base_url,
            commands::init::admin_user_id,
            logging::log_debug,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
