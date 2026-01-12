//! Traffic logging for LLM API calls
//!
//! Logs LLM errors only. Request/response content is not logged
//! to protect user privacy (may contain blobs, personal data).

use config::PathManager;
use std::io::Write;

/// Log an LLM request (no-op, content not logged for privacy)
pub fn log_request(_model: &str, _request: &impl serde::Serialize) {}

/// Log an LLM response (no-op, content not logged for privacy)
pub fn log_response(_model: &str, _response: &impl serde::Serialize) {}

/// Log an LLM error
pub fn log_error(model: &str, error: &str) {
    log_traffic("ERROR", &format!("[{}] {}", model, error));
}

/// Log an LLM streaming start (no-op, content not logged for privacy)
pub fn log_stream_start(_model: &str, _request: &impl serde::Serialize) {}

/// Log an LLM streaming response (no-op, content not logged for privacy)
pub fn log_stream_response(_model: &str, _response: &impl serde::Serialize) {}

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
