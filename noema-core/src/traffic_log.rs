//! Traffic logging for LLM and MCP calls
//!
//! Logs all LLM requests/responses and MCP tool calls to noema.log

use config::PathManager;
use std::io::Write;

/// Log an LLM request
pub fn log_llm_request(model: &str, request: &impl serde::Serialize) {
    let json = serde_json::to_string_pretty(request).unwrap_or_else(|_| "<serialization error>".to_string());
    log_traffic("LLM", "REQUEST", &format!("[{}]\n{}", model, json));
}

/// Log an LLM response
pub fn log_llm_response(model: &str, response: &impl serde::Serialize) {
    let json = serde_json::to_string_pretty(response).unwrap_or_else(|_| "<serialization error>".to_string());
    log_traffic("LLM", "RESPONSE", &format!("[{}]\n{}", model, json));
}

/// Log an LLM error
pub fn log_llm_error(model: &str, error: &str) {
    log_traffic("LLM", "ERROR", &format!("[{}] {}", model, error));
}

/// Log an LLM streaming start
pub fn log_llm_stream_start(model: &str, request: &impl serde::Serialize) {
    let json = serde_json::to_string_pretty(request).unwrap_or_else(|_| "<serialization error>".to_string());
    log_traffic("LLM", "STREAM_START", &format!("[{}]\n{}", model, json));
}

/// Log an MCP tool call request
pub fn log_mcp_request(tool_name: &str, args: &serde_json::Value) {
    let json = serde_json::to_string_pretty(args).unwrap_or_else(|_| "<serialization error>".to_string());
    log_traffic("MCP", "REQUEST", &format!("[{}]\n{}", tool_name, json));
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
