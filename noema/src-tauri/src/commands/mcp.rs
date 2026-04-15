//! MCP (Model Context Protocol) server commands.
//!
//! Thin wrappers around `McpApi` trait methods on the daemon.

use simply_daemon::api::{Daemon, McpApi, OAuthApi, UpdateMcpServerRequest};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;
use crate::types::{AddMcpServerRequest, McpServerInfo, McpToolInfo};

#[tauri::command]
pub async fn list_mcp_servers(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<McpServerInfo>, String> {
    let daemon = state.get_daemon()?;
    let servers = daemon.mcp().list_mcp_servers().await.map_err(|e| e.to_string())?;
    Ok(servers.into_iter().map(McpServerInfo::from).collect())
}

#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, Arc<AppState>>,
    request: AddMcpServerRequest,
) -> Result<(), String> {
    let daemon = state.get_daemon()?;
    daemon.mcp()
        .add_mcp_server(request.into())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_mcp_server(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<(), String> {
    let daemon = state.get_daemon()?;
    daemon.mcp()
        .remove_mcp_server(&server_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn connect_mcp_server(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<usize, String> {
    let daemon = state.get_daemon()?;
    daemon.mcp()
        .connect_mcp_server(&server_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn disconnect_mcp_server(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<(), String> {
    let daemon = state.get_daemon()?;
    daemon.mcp()
        .disconnect_mcp_server(&server_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_mcp_server_tools(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<McpToolInfo>, String> {
    let daemon = state.get_daemon()?;
    let tools = daemon.mcp()
        .get_mcp_server_tools(&server_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(tools.into_iter().map(McpToolInfo::from).collect())
}

#[tauri::command]
pub async fn update_mcp_server_settings(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    auto_connect: bool,
    auto_retry: bool,
) -> Result<(), String> {
    let daemon = state.get_daemon()?;
    daemon.mcp()
        .update_mcp_server_settings(
            &server_id,
            UpdateMcpServerRequest {
                name: None,
                url: None,
                auto_connect: Some(auto_connect),
                auto_retry: Some(auto_retry),
            },
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_mcp_retry(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<(), String> {
    let daemon = state.get_daemon()?;
    daemon.mcp()
        .stop_mcp_retry(&server_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_mcp_retry(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<(), String> {
    let daemon = state.get_daemon()?;
    daemon.mcp()
        .start_mcp_retry(&server_id)
        .await
        .map_err(|e| e.to_string())
}

/// Start OAuth flow for an MCP server. Returns the authorization URL.
#[tauri::command]
pub async fn start_mcp_oauth(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<String, String> {
    let daemon = state.get_daemon()?;
    let flow = daemon.oauth()
        .start_oauth(&server_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(flow.auth_url)
}

/// Complete OAuth flow with an authorization code (manual entry from frontend).
#[tauri::command]
pub async fn complete_mcp_oauth(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    code: String,
) -> Result<(), String> {
    let daemon = state.get_daemon()?;
    // Manual code entry — use the code-only completion path
    daemon.oauth()
        .complete_oauth_with_code(&server_id, &code)
        .await
        .map_err(|e| e.to_string())
}

/// Handle incoming deep link URLs (e.g., noema://oauth/callback?code=...&state=...).
pub async fn handle_deep_link(app: &AppHandle, urls: Vec<url::Url>) {
    let state: tauri::State<'_, Arc<AppState>> = app.state();
    let daemon = match state.get_daemon() {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(error = %e, "Deep link received but daemon not initialized");
            return;
        }
    };

    for url in urls {
        tracing::info!(url = %url, "Deep link received");

        let is_oauth_callback = url.scheme() == "noema"
            && url.host_str() == Some("oauth")
            && url.path() == "/callback";

        if !is_oauth_callback {
            continue;
        }

        let code = url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, value)| value.to_string());

        let oauth_state = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.to_string());

        let (Some(code), Some(oauth_state)) = (code, oauth_state) else {
            tracing::warn!("Incomplete OAuth callback — missing code or state");
            continue;
        };

        // Resolve which server this state belongs to
        let Some(server_id) = daemon.oauth().resolve_oauth_state(&oauth_state).await else {
            tracing::warn!(state = %oauth_state, "No pending OAuth flow for state");
            app.emit("oauth_error", "No pending OAuth flow found").ok();
            continue;
        };

        match daemon.oauth().complete_oauth(&server_id, &code, &oauth_state).await {
            Ok(()) => {
                tracing::info!(server_id, "OAuth completed via deep link");
                app.emit("oauth_complete", &server_id).ok();
            }
            Err(e) => {
                tracing::error!(error = %e, "OAuth deep link completion failed");
                app.emit("oauth_error", e.to_string()).ok();
            }
        }
    }
}
