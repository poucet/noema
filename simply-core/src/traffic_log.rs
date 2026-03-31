//! Traffic logging for LLM and MCP calls
//!
//! Logs all LLM requests/responses and MCP tool calls to noema.log
//! Content is truncated to avoid leaking private data in logs.

use config::PathManager;
use std::io::Write;

/// Maximum characters to log for content (to protect privacy)
const MAX_CONTENT_LOG_CHARS: usize = 200;

/// Truncate a string for logging, adding ellipsis if truncated
fn truncate_for_log(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}... ({} chars total)", &s[..max_len], s.len())
    }
}

/// Log an LLM request (truncated summary only)
pub fn log_llm_request(model: &str, request: &impl serde::Serialize) {
    let json = serde_json::to_string(request).unwrap_or_else(|_| "<serialization error>".to_string());
    let summary = truncate_for_log(&json, MAX_CONTENT_LOG_CHARS);
    log_traffic("LLM", "REQUEST", &format!("[{}] {}", model, summary));
}

/// Log an LLM response (truncated summary only)
pub fn log_llm_response(model: &str, response: &impl serde::Serialize) {
    let json = serde_json::to_string(response).unwrap_or_else(|_| "<serialization error>".to_string());
    let summary = truncate_for_log(&json, MAX_CONTENT_LOG_CHARS);
    log_traffic("LLM", "RESPONSE", &format!("[{}] {}", model, summary));
}

/// Log an LLM error
pub fn log_llm_error(model: &str, error: &str) {
    log_traffic("LLM", "ERROR", &format!("[{}] {}", model, error));
}

/// Log an LLM streaming start (truncated summary only)
pub fn log_llm_stream_start(model: &str, request: &impl serde::Serialize) {
    let json = serde_json::to_string(request).unwrap_or_else(|_| "<serialization error>".to_string());
    let summary = truncate_for_log(&json, MAX_CONTENT_LOG_CHARS);
    log_traffic("LLM", "STREAM_START", &format!("[{}] {}", model, summary));
}

/// Log an LLM streaming end
pub fn log_llm_stream_end(model: &str, chunk_count: u64, total_chars: usize) {
    log_traffic("LLM", "STREAM_END", &format!("[{}] chunks={}, chars={}", model, chunk_count, total_chars));
}

/// Log an MCP tool call request (truncated summary only)
pub fn log_mcp_request(tool_name: &str, args: &serde_json::Value) {
    let json = serde_json::to_string(args).unwrap_or_else(|_| "<serialization error>".to_string());
    let summary = truncate_for_log(&json, MAX_CONTENT_LOG_CHARS);
    log_traffic("MCP", "REQUEST", &format!("[{}] {}", tool_name, summary));
}

/// Log an MCP tool call response
pub fn log_mcp_response(tool_name: &str, result: &[llm::ToolResultContent]) {
    let summary: Vec<String> = result.iter().map(|c| match c {
        llm::ToolResultContent::Text { text } => {
            if text.len() > 500 {
                format!("text({} chars): {}...", text.len(), &text[..200])
            } else {
                format!("text: {}", text)
            }
        }
        llm::ToolResultContent::Image { mime_type, .. } => format!("image({})", mime_type),
        llm::ToolResultContent::Audio { mime_type, .. } => format!("audio({})", mime_type),
    }).collect();
    log_traffic("MCP", "RESPONSE", &format!("[{}] {:?}", tool_name, summary));
}

/// Log an MCP tool call error
pub fn log_mcp_error(tool_name: &str, error: &str) {
    log_traffic("MCP", "ERROR", &format!("[{}] {}", tool_name, error));
}

/// Internal function to write to the log file
fn log_traffic(category: &str, event_type: &str, message: &str) {
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
            let _ = writeln!(file, "[{}] [TRAFFIC] [{}] [{}] {}", timestamp, category, event_type, message);
        }
    }
}
