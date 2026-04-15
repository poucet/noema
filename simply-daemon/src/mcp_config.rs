//! MCP config file I/O.
//!
//! Loads and saves `McpConfig` from `~/.config/noema/mcp.toml`.
//! simply-core's `McpConfig` is a pure in-memory type — this module
//! adds the file persistence layer.

use simply_core::mcp::McpConfig;

/// Load MCP config from the default path, or return empty config.
pub fn load_mcp_config() -> McpConfig {
    let Some(path) = config::PathManager::mcp_config_path() else {
        return McpConfig::default();
    };
    if !path.exists() {
        return McpConfig::default();
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return McpConfig::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// Save MCP config to the default path.
pub fn save_mcp_config(config: &McpConfig) -> anyhow::Result<()> {
    let path = config::PathManager::mcp_config_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine MCP config path"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}
