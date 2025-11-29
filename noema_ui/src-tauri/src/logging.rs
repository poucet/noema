//! Logging utilities for Noema

use std::io::Write;

/// Log a message to ~/Library/Logs/Noema/noema.log
pub fn log_message(msg: &str) {
    if let Some(log_dir) = dirs::home_dir().map(|h| h.join("Library/Logs/Noema")) {
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("noema.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] {}", timestamp, msg);
        }
    }
}

/// Frontend logging command - allows JS to write to the same log file
#[tauri::command]
pub fn log_debug(level: String, source: String, message: String) {
    let formatted = format!("[{}] [{}] {}", level.to_uppercase(), source, message);
    log_message(&formatted);
}
