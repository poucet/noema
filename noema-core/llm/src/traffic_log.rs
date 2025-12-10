//! Traffic logging for LLM API calls
//!
//! Logs all LLM requests/responses to noema.log

use config::PathManager;
use std::io::Write;

/// Log an LLM request
pub fn log_request(model: &str, request: &impl serde::Serialize) {
    let json = serde_json::to_string_pretty(request).unwrap_or_else(|_| "<serialization error>".to_string());
    log_traffic("REQUEST", &format!("[{}]\n{}", model, json));
}

/// Log an LLM response
pub fn log_response(model: &str, response: &impl serde::Serialize) {
    let json = serde_json::to_string_pretty(response).unwrap_or_else(|_| "<serialization error>".to_string());
    log_traffic("RESPONSE", &format!("[{}]\n{}", model, json));
}

/// Log an LLM error
pub fn log_error(model: &str, error: &str) {
    log_traffic("ERROR", &format!("[{}] {}", model, error));
}

/// Log an LLM streaming start
pub fn log_stream_start(model: &str, request: &impl serde::Serialize) {
    let json = serde_json::to_string_pretty(request).unwrap_or_else(|_| "<serialization error>".to_string());
    log_traffic("STREAM_START", &format!("[{}]\n{}", model, json));
}

/// Internal function to write to the log file
fn log_traffic(event_type: &str, message: &str) {
    if let Some(log_path) = PathManager::log_file_path() {
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(file, "[{}] [TRAFFIC] [LLM] [{}] {}", timestamp, event_type, message);
        }
    }
}
