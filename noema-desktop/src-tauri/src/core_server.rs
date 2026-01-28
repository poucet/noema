//! Embedded Noema Core MCP server management
//!
//! Starts the noema-mcp-core server in-process and registers it
//! as an ephemeral server in the MCP registry.
//!
//! The server is stateless - context is injected via the enricher callback
//! in McpAgent (see manager.rs create_noema_core_enricher).

use crate::logging::log_message;
use crate::state::AppCoordinator;
use noema_core::storage::DocumentResolver;
use noema_core::McpRegistry;
use noema_mcp_core::{NoemaCoreServer, ServerHandle};
use std::sync::Arc;
use tokio::sync::Mutex;

/// State for the embedded Noema Core server
pub struct CoreServerState {
    handle: Arc<Mutex<Option<ServerHandle>>>,
}

impl Default for CoreServerState {
    fn default() -> Self {
        Self {
            handle: Arc::new(Mutex::new(None)),
        }
    }
}

impl CoreServerState {
    /// Get the URL of the running server
    pub async fn url(&self) -> Option<String> {
        self.handle.lock().await.as_ref().map(|h| h.url())
    }

    /// Get the port of the running server
    pub async fn port(&self) -> Option<u16> {
        self.handle.lock().await.as_ref().map(|h| h.port())
    }
}

/// Start the embedded Noema Core MCP server
pub async fn start_core_server(
    server_state: &CoreServerState,
    coordinator: Arc<AppCoordinator>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    document_resolver: Arc<dyn DocumentResolver>,
) -> Result<String, String> {
    // Check if already running
    if server_state.handle.lock().await.is_some() {
        let url = server_state.url().await.unwrap_or_default();
        log_message(&format!("Noema Core server already running at {}", url));
        return Ok(url);
    }

    log_message("Starting embedded Noema Core MCP server...");

    // Create the server (stateless - context is injected via enricher in McpAgent)
    let server = NoemaCoreServer::new(
        coordinator,
        Arc::clone(&mcp_registry),
        document_resolver,
    );

    // Start the server on a random port
    let handle = noema_mcp_core::start_server(server)
        .await
        .map_err(|e| format!("Failed to start Noema Core server: {}", e))?;

    let url = handle.url();
    let port = handle.port();

    log_message(&format!("Noema Core MCP server started on port {}", port));

    // Store the handle
    *server_state.handle.lock().await = Some(handle);

    // Register as ephemeral server in MCP registry
    {
        let mut registry = mcp_registry.lock().await;
        registry.register_ephemeral("noema-core".to_string(), url.clone());
        log_message(&format!("Registered noema-core ephemeral server at {}", url));
    }

    // Auto-connect the server
    {
        let mut registry = mcp_registry.lock().await;
        if let Err(e) = registry.connect("noema-core").await {
            log_message(&format!("Warning: Failed to auto-connect noema-core: {}", e));
        } else {
            log_message("noema-core MCP server connected");
        }
    }

    Ok(url)
}

/// Stop the embedded Noema Core MCP server
pub async fn stop_core_server(
    server_state: &CoreServerState,
    mcp_registry: Arc<Mutex<McpRegistry>>,
) -> Result<(), String> {
    // Unregister from registry
    {
        let mut registry = mcp_registry.lock().await;
        registry.unregister_ephemeral("noema-core").await;
    }

    // Stop the server
    if let Some(handle) = server_state.handle.lock().await.take() {
        log_message("Stopping Noema Core MCP server");
        handle.stop();
    }

    Ok(())
}
