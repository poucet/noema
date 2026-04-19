//! MCP config file I/O.
//!
//! Loads and saves `DaemonMcpConfig` (auth + connection config) from
//! `~/.config/noema/mcp.toml`. The daemon splits this at runtime into
//! `simply_core::McpConfig` (connections) + auth state.

use crate::mcp::auth::DaemonMcpConfig;

/// Load daemon MCP config from the default path, or return empty config.
pub fn load_mcp_config() -> DaemonMcpConfig {
    let Some(path) = config::PathManager::mcp_config_path() else {
        return DaemonMcpConfig::default();
    };
    if !path.exists() {
        return DaemonMcpConfig::default();
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return DaemonMcpConfig::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// Save daemon MCP config to the default path.
pub fn save_mcp_config(config: &DaemonMcpConfig) -> anyhow::Result<()> {
    let path = config::PathManager::mcp_config_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine MCP config path"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}
