//! Unified logging for Noema using tracing
//!
//! All logs (backend tracing + frontend log_debug) go to:
//! ~/.local/share/noema/logs/noema.log (or platform equivalent)

use config::PathManager;
use std::sync::Once;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static INIT: Once = Once::new();
static mut LOG_GUARD: Option<WorkerGuard> = None;

/// Initialize the unified tracing subscriber
/// This should be called once at app startup
pub fn init_logging() {
    INIT.call_once(|| {
        let log_path = PathManager::log_file_path();
        eprintln!("[noema] init_logging: log_path = {:?}", log_path);

        if let Some(path) = log_path {
            // Create parent directory if needed
            if let Some(parent) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!("[noema] Failed to create log directory {:?}: {}", parent, e);
                }
            }

            // Create a file appender
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path);

            match file {
                Ok(file) => {
                    eprintln!("[noema] Opened log file: {:?}", path);
                    let (non_blocking, guard) = tracing_appender::non_blocking(file);

                    // Store guard to keep writer alive
                    // SAFETY: Only called once due to Once guard
                    unsafe {
                        LOG_GUARD = Some(guard);
                    }

                    // Build subscriber with file output
                    // Include all noema crates at debug level
                    let filter = EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("info,noema_ui_lib=debug,noema_core=info,noema_mcp_gdocs=debug"));

                    let subscriber = tracing_subscriber::registry()
                        .with(filter)
                        .with(
                            fmt::layer()
                                .with_writer(non_blocking)
                                .with_ansi(false)
                                .with_target(true)
                                .with_thread_ids(false)
                                .with_file(true)
                                .with_line_number(true),
                        );

                    match tracing::subscriber::set_global_default(subscriber) {
                        Ok(()) => {
                            eprintln!("[noema] Tracing subscriber installed successfully");
                            tracing::info!("Logging initialized, writing to {:?}", path);
                        }
                        Err(e) => {
                            eprintln!("[noema] Failed to set tracing subscriber: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[noema] Failed to open log file {:?}: {}", path, e);
                    init_stderr_logging();
                }
            }
        } else {
            eprintln!("[noema] No log path configured, using stderr");
            init_stderr_logging();
        }
    });
}

fn init_stderr_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,noema_ui_lib=debug,noema_core=debug,noema_mcp_gdocs=debug"));

    let subscriber = tracing_subscriber::registry().with(filter).with(
        fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_target(true),
    );

    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Log a message to the log file (legacy function for compatibility)
/// New code should use tracing macros directly
pub fn log_message(msg: &str) {
    // Use tracing only - the tracing subscriber already writes to the log file
    tracing::info!("{}", msg);
}

/// Frontend logging command - allows JS to write to the same log file
#[tauri::command]
pub fn log_debug(level: String, source: String, message: String) {
    match level.to_lowercase().as_str() {
        "error" => tracing::error!(target: "frontend", source = %source, "{}", message),
        "warn" => tracing::warn!(target: "frontend", source = %source, "{}", message),
        "debug" => tracing::debug!(target: "frontend", source = %source, "{}", message),
        "trace" => tracing::trace!(target: "frontend", source = %source, "{}", message),
        _ => tracing::info!(target: "frontend", source = %source, "{}", message),
    }
}
