//! Traffic logging for LLM API calls
//!
//! Logs all LLM requests/responses to noema.log
//! Content is truncated to avoid leaking private data in logs.

use config::PathManager;
use std::io::Write;

/// Maximum characters to log for content (to protect privacy)
const MAX_CONTENT_LOG_CHARS: usize = 200;

/// Truncate a string for logging, adding ellipsis if truncated
fn truncate_for_log(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}... ({} chars total)", truncated, char_count)
    }
}

/// Log an LLM request (truncated summary only)
pub fn log_request(model: &str, request: &impl serde::Serialize) {
    let json = serde_json::to_string(request).unwrap_or_else(|_| "<serialization error>".to_string());
    let summary = truncate_for_log(&json, MAX_CONTENT_LOG_CHARS);
    log_traffic("REQUEST", &format!("[{}] {}", model, summary));
}

/// Log an LLM response (truncated summary only)
pub fn log_response(model: &str, response: &impl serde::Serialize) {
    let json = serde_json::to_string(response).unwrap_or_else(|_| "<serialization error>".to_string());
    let summary = truncate_for_log(&json, MAX_CONTENT_LOG_CHARS);
    log_traffic("RESPONSE", &format!("[{}] {}", model, summary));
}

/// Log an LLM error
pub fn log_error(model: &str, error: &str) {
    log_traffic("ERROR", &format!("[{}] {}", model, error));
}

/// Log an LLM streaming start (truncated summary only)
pub fn log_stream_start(model: &str, request: &impl serde::Serialize) {
    let json = serde_json::to_string(request).unwrap_or_else(|_| "<serialization error>".to_string());
    let summary = truncate_for_log(&json, MAX_CONTENT_LOG_CHARS);
    log_traffic("STREAM_START", &format!("[{}] {}", model, summary));
}

/// Log an LLM streaming response (truncated summary only)
pub fn log_stream_response(model: &str, response: &impl serde::Serialize) {
    let json = serde_json::to_string(response).unwrap_or_else(|_| "<serialization error>".to_string());
    let summary = truncate_for_log(&json, MAX_CONTENT_LOG_CHARS);
    log_traffic("STREAM_RESPONSE", &format!("[{}] {}", model, summary));
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
