//! Embedded Google Docs MCP server management
//!
//! Starts the Google Docs MCP server in-process and registers it
//! with the MCP configuration.

use crate::logging::log_message;
use noema_core::mcp::{AuthMethod, McpConfig, ServerConfig};
use noema_mcp_gdocs::ServerHandle;
use std::sync::Arc;
use tokio::sync::Mutex;

/// State for the embedded Google Docs server
pub struct GDocsServerState {
    handle: Arc<Mutex<Option<ServerHandle>>>,
}

impl Default for GDocsServerState {
    fn default() -> Self {
        Self {
            handle: Arc::new(Mutex::new(None)),
        }
    }
}

impl GDocsServerState {
    /// Get the URL of the running server
    pub async fn url(&self) -> Option<String> {
        self.handle.lock().await.as_ref().map(|h| h.url())
    }

    /// Get the port of the running server
    pub async fn port(&self) -> Option<u16> {
        self.handle.lock().await.as_ref().map(|h| h.port())
    }
}

/// Start the embedded Google Docs MCP server
pub async fn start_gdocs_server(state: &GDocsServerState) -> Result<String, String> {
    // Check if already running
    if state.handle.lock().await.is_some() {
        let url = state.url().await.unwrap_or_default();
        log_message(&format!("Google Docs server already running at {}", url));
        return Ok(url);
    }

    log_message("Starting embedded Google Docs MCP server...");

    // Start the server on a random port
    let handle = noema_mcp_gdocs::start_server()
        .await
        .map_err(|e| format!("Failed to start Google Docs server: {}", e))?;

    let url = handle.url();
    let port = handle.port();

    log_message(&format!("Google Docs MCP server started on port {}", port));

    // Store the handle
    *state.handle.lock().await = Some(handle);

    // Register in MCP config
    register_gdocs_server(&url).await?;

    Ok(url)
}

/// Register the Google Docs MCP server in the config
async fn register_gdocs_server(server_url: &str) -> Result<(), String> {
    let mut config = McpConfig::load().unwrap_or_default();

    // Check if already registered
    if let Some(existing) = config.get_server("gdocs") {
        if existing.url == server_url {
            log_message("gdocs server already registered with correct URL");
            return Ok(());
        }
        // URL changed (new port) - update URL but preserve credentials
        log_message(&format!("Updating gdocs server URL to {}", server_url));
        let updated_config = ServerConfig {
            url: server_url.to_string(),
            ..existing.clone()
        };
        config.add_server("gdocs".to_string(), updated_config);
        config.save().map_err(|e| format!("Failed to save MCP config: {}", e))?;
        return Ok(());
    }

    // First time registration - create with empty credentials
    // User will configure client_id via the Google Docs settings UI
    let server_config = ServerConfig {
        name: "Google Docs".to_string(),
        url: server_url.to_string(),
        auth: AuthMethod::OAuth {
            client_id: String::new(),
            client_secret: None,
            authorization_url: Some("https://accounts.google.com/o/oauth2/v2/auth".to_string()),
            token_url: Some("https://oauth2.googleapis.com/token".to_string()),
            scopes: vec![
                "https://www.googleapis.com/auth/drive.readonly".to_string(),
                "https://www.googleapis.com/auth/documents.readonly".to_string(),
            ],
            access_token: None,
            refresh_token: None,
            expires_at: None,
        },
        use_well_known: true,
        auth_token: None,
        auto_connect: false, // Don't auto-connect until OAuth is configured
        auto_retry: true,
    };

    config.add_server("gdocs".to_string(), server_config);
    config.save().map_err(|e| format!("Failed to save MCP config: {}", e))?;

    log_message(&format!("Registered gdocs MCP server at {}", server_url));
    Ok(())
}

/// Stop the embedded Google Docs MCP server
pub async fn stop_gdocs_server(state: &GDocsServerState) -> Result<(), String> {
    if let Some(handle) = state.handle.lock().await.take() {
        log_message("Stopping Google Docs MCP server");
        handle.stop();
    }
    Ok(())
}
