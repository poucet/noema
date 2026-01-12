//! MCP (Model Context Protocol) server commands

use noema_core::mcp::{spawn_retry_task, ServerStatus};
use noema_core::{AuthMethod, ServerConfig};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::logging::log_message;
use crate::oauth_callback;
use crate::state::{save_pending_oauth_states, AppState};
use crate::types::{AddMcpServerRequest, McpServerInfo, McpToolInfo};

/// List all configured MCP servers
#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, Arc<AppState>>) -> Result<Vec<McpServerInfo>, String> {
    let mcp_registry = state.get_mcp_registry()?;
    let registry = mcp_registry.lock().await;

    let mut servers = Vec::new();
    for (id, config) in registry.list_servers() {
        let is_connected = registry.is_connected(id);
        let tool_count = if let Some(conn) = registry.get_connection(id) {
            conn.tools.len()
        } else {
            0
        };

        let auth_type = match &config.auth {
            AuthMethod::None => "none",
            AuthMethod::Token { .. } => "token",
            AuthMethod::OAuth { .. } => "oauth",
        };

        // Get server status
        let server_status = registry.get_status(id);
        let status = match &server_status {
            ServerStatus::Disconnected => "disconnected".to_string(),
            ServerStatus::Connected => "connected".to_string(),
            ServerStatus::Retrying { attempt } => format!("retrying:{}", attempt),
            ServerStatus::RetryStopped { last_error } => format!("stopped:{}", last_error),
        };

        servers.push(McpServerInfo {
            id: id.to_string(),
            name: config.name.clone(),
            url: config.url.clone(),
            auth_type: auth_type.to_string(),
            is_connected,
            needs_oauth_login: config.auth.needs_oauth_login(),
            tool_count,
            status,
            auto_connect: config.auto_connect,
            auto_retry: config.auto_retry,
        });
    }

    Ok(servers)
}

/// Add a new MCP server configuration
/// If auth_type is not specified or is "auto", probe .well-known to detect OAuth
#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, Arc<AppState>>,
    request: AddMcpServerRequest,
) -> Result<(), String> {
    let auth = match request.auth_type.as_str() {
        "token" => AuthMethod::Token {
            token: request.token.ok_or("Token required for token auth")?,
        },
        "oauth" => {
            // Explicitly requested OAuth
            AuthMethod::OAuth {
                client_id: request.client_id.unwrap_or_else(|| "noema".to_string()),
                client_secret: request.client_secret,
                authorization_url: None,
                token_url: None,
                scopes: request.scopes.unwrap_or_default(),
                access_token: None,
                refresh_token: None,
                expires_at: None,
            }
        }
        "none" => AuthMethod::None,
        _ => {
            // Auto-detect: probe .well-known to see if OAuth is available
            log_message(&format!(
                "Auto-detecting auth for server: {}",
                request.url
            ));
            if let Ok(well_known) = fetch_well_known(&request.url).await {
                if well_known.get("authorization_endpoint").is_some() {
                    log_message("OAuth detected via .well-known");
                    AuthMethod::OAuth {
                        client_id: "noema".to_string(),
                        client_secret: None,
                        authorization_url: None,
                        token_url: None,
                        scopes: vec![],
                        access_token: None,
                        refresh_token: None,
                        expires_at: None,
                    }
                } else {
                    log_message("No OAuth in .well-known, using no auth");
                    AuthMethod::None
                }
            } else {
                log_message("No .well-known found, using no auth");
                AuthMethod::None
            }
        }
    };

    let use_well_known = matches!(auth, AuthMethod::OAuth { .. });

    let config = ServerConfig {
        name: request.name,
        url: request.url,
        auth,
        use_well_known,
        auth_token: None,
        auto_connect: true,
        auto_retry: true,
    };

    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;
    registry.add_server(request.id, config);
    registry.save_config().map_err(|e| e.to_string())?;

    Ok(())
}

/// Remove an MCP server configuration
#[tauri::command]
pub async fn remove_mcp_server(state: State<'_, Arc<AppState>>, server_id: String) -> Result<(), String> {
    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;
    registry
        .remove_server(&server_id)
        .await
        .map_err(|e| e.to_string())?;
    registry.save_config().map_err(|e| e.to_string())?;

    Ok(())
}

/// Connect to an MCP server
#[tauri::command]
pub async fn connect_mcp_server(state: State<'_, Arc<AppState>>, server_id: String) -> Result<usize, String> {
    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;

    let server = registry
        .connect(&server_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(server.tools.len())
}

/// Disconnect from an MCP server
#[tauri::command]
pub async fn disconnect_mcp_server(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<(), String> {
    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;
    registry
        .disconnect(&server_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Get tools from a connected MCP server
#[tauri::command]
pub async fn get_mcp_server_tools(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<McpToolInfo>, String> {
    let mcp_registry = state.get_mcp_registry()?;
    let registry = mcp_registry.lock().await;

    let server = registry
        .get_connection(&server_id)
        .ok_or("Server not connected")?;

    let tools = server
        .tools
        .iter()
        .map(|tool| McpToolInfo {
            name: tool.name.to_string(),
            description: tool.description.as_ref().map(|d| d.to_string()),
            server_id: server_id.clone(),
        })
        .collect();

    Ok(tools)
}

/// Test connection to an MCP server (connect and immediately disconnect)
#[tauri::command]
pub async fn test_mcp_server(state: State<'_, Arc<AppState>>, server_id: String) -> Result<usize, String> {
    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;

    // Connect to test
    let server = registry
        .connect(&server_id)
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    let tool_count = server.tools.len();

    // Disconnect after test
    registry
        .disconnect(&server_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(tool_count)
}

/// Fetch .well-known OAuth configuration
pub async fn fetch_well_known(base_url: &str) -> Result<serde_json::Value, String> {
    let base = url::Url::parse(base_url).map_err(|e| format!("Invalid server URL: {}", e))?;
    let well_known_url = base
        .join("/.well-known/oauth-authorization-server")
        .map_err(|e| format!("Failed to construct well-known URL: {}", e))?;

    let client = reqwest::Client::new();
    let resp = client
        .get(well_known_url.as_str())
        .send()
        .await
        .map_err(|e| format!("Failed to fetch well-known config: {}", e))?;

    resp.json()
        .await
        .map_err(|e| format!("Failed to parse well-known config: {}", e))
}

/// Perform dynamic client registration (RFC 7591)
async fn register_oauth_client(
    registration_endpoint: &str,
    redirect_uri: &str,
) -> Result<(String, Option<String>), String> {
    let client = reqwest::Client::new();

    let registration_request = serde_json::json!({
        "client_name": "Noema",
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none"  // Public client
    });

    let resp = client
        .post(registration_endpoint)
        .json(&registration_request)
        .send()
        .await
        .map_err(|e| format!("Client registration failed: {}", e))?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        return Err(format!("Client registration failed: {}", error_text));
    }

    let registration_response: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse registration response: {}", e))?;

    let client_id = registration_response["client_id"]
        .as_str()
        .ok_or("No client_id in registration response")?
        .to_string();

    let client_secret = registration_response["client_secret"]
        .as_str()
        .map(String::from);

    Ok((client_id, client_secret))
}

/// Start OAuth flow for an MCP server (returns authorization URL)
///
/// This function:
/// 1. Starts a local HTTP server to receive the OAuth callback
/// 2. Returns the authorization URL for the user to open in their browser
/// 3. Spawns a background task to wait for the callback and complete the token exchange
#[tauri::command]
pub async fn start_mcp_oauth(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<String, String> {
    let mcp_registry = state.get_mcp_registry()?;
    let config = {
        let registry = mcp_registry.lock().await;
        registry
            .config()
            .get_server(&server_id)
            .ok_or("Server not found")?
            .clone()
    };

    match &config.auth {
        AuthMethod::OAuth {
            client_id,
            client_secret,
            authorization_url,
            token_url,
            scopes,
            ..
        } => {
            // Start local callback server
            let callback_server = oauth_callback::start_callback_server()
                .await
                .map_err(|e| format!("Failed to start callback server: {}", e))?;

            let redirect_uri = callback_server.redirect_uri();
            log_message(&format!("OAuth callback server started at {}", redirect_uri));

            // Fetch .well-known config if needed
            let well_known = if config.use_well_known {
                Some(fetch_well_known(&config.url).await?)
            } else {
                None
            };

            // Get authorization URL
            let auth_url = if let Some(url) = authorization_url {
                url.clone()
            } else if let Some(ref wk) = well_known {
                wk["authorization_endpoint"]
                    .as_str()
                    .ok_or("No authorization_endpoint in well-known config")?
                    .to_string()
            } else {
                return Err("OAuth requires authorization_url or use_well_known".to_string());
            };

            // Get token URL for later use
            let tok_url = if let Some(url) = token_url {
                url.clone()
            } else if let Some(ref wk) = well_known {
                wk["token_endpoint"]
                    .as_str()
                    .ok_or("No token_endpoint in well-known config")?
                    .to_string()
            } else {
                return Err("OAuth requires token_url or use_well_known".to_string());
            };

            // Check if we need to register the client dynamically
            let needs_registration = client_id == "noema" || client_id.is_empty();

            if needs_registration {
                // For Google OAuth, we don't support dynamic registration
                // The user must configure client_id manually
                return Err("Please configure your Google OAuth Client ID in the Google Docs settings first.".to_string());
            }

            let final_client_id = client_id.clone();
            let final_client_secret = client_secret.clone();

            // Build authorization URL with state parameter
            let state_param = uuid::Uuid::new_v4().to_string();

            // Store the state -> server_id mapping
            {
                let mut pending_states = state.pending_oauth_states.lock().await;
                pending_states.insert(state_param.clone(), server_id.clone());
                if let Err(e) = save_pending_oauth_states(&pending_states) {
                    log_message(&format!("Warning: Failed to persist OAuth state: {}", e));
                }
            }

            let scope_str = if scopes.is_empty() {
                "openid".to_string()
            } else {
                scopes.join(" ")
            };

            let mut url =
                url::Url::parse(&auth_url).map_err(|e| format!("Invalid authorization URL: {}", e))?;

            url.query_pairs_mut()
                .append_pair("client_id", &final_client_id)
                .append_pair("response_type", "code")
                .append_pair("redirect_uri", &redirect_uri)
                .append_pair("state", &state_param)
                .append_pair("scope", &scope_str)
                .append_pair("access_type", "offline") // Request refresh token
                .append_pair("prompt", "consent"); // Force consent to get refresh token

            let auth_url_str = url.to_string();

            // Clone values for the background task
            let app_clone = app.clone();
            let server_id_clone = server_id.clone();
            let state_param_clone = state_param.clone();
            let redirect_uri_clone = redirect_uri.clone();
            let scopes_clone = scopes.clone();
            let config_clone = config.clone();

            // Spawn background task to handle the callback
            tokio::spawn(async move {
                log_message("Waiting for OAuth callback...");

                match callback_server.wait_for_callback().await {
                    Ok((code, received_state)) => {
                        log_message(&format!("Received OAuth callback with state: {}", received_state));

                        // Verify state matches
                        if received_state != state_param_clone {
                            log_message("OAuth state mismatch!");
                            let _ = app_clone.emit("oauth_error", "State parameter mismatch");
                            return;
                        }

                        // Exchange code for tokens
                        match exchange_code_for_tokens(
                            &tok_url,
                            &code,
                            &redirect_uri_clone,
                            &final_client_id,
                            final_client_secret.as_deref(),
                        )
                        .await
                        {
                            Ok((access_token, refresh_token, expires_in)) => {
                                log_message("Successfully exchanged code for tokens");

                                // Update the MCP config with the new tokens
                                if let Err(e) = save_oauth_tokens(
                                    &app_clone,
                                    &server_id_clone,
                                    &config_clone,
                                    access_token,
                                    refresh_token,
                                    expires_in,
                                    &scopes_clone,
                                )
                                .await
                                {
                                    log_message(&format!("Failed to save OAuth tokens: {}", e));
                                    let _ = app_clone.emit("oauth_error", e);
                                    return;
                                }

                                // Emit success event
                                let _ = app_clone.emit("oauth_complete", server_id_clone);
                            }
                            Err(e) => {
                                log_message(&format!("Token exchange failed: {}", e));
                                let _ = app_clone.emit("oauth_error", e);
                            }
                        }
                    }
                    Err(e) => {
                        log_message(&format!("OAuth callback failed: {}", e));
                        let _ = app_clone.emit("oauth_error", e);
                    }
                }
            });

            Ok(auth_url_str)
        }
        _ => Err("Server is not configured for OAuth".to_string()),
    }
}

/// Exchange authorization code for access and refresh tokens
async fn exchange_code_for_tokens(
    token_url: &str,
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<(String, Option<String>, Option<i64>), String> {
    let http_client = reqwest::Client::new();

    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
    ];

    if let Some(secret) = client_secret {
        params.push(("client_secret", secret));
    }

    let resp = http_client
        .post(token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {}", e))?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {}", error_text));
    }

    let token_response: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    let access_token = token_response["access_token"]
        .as_str()
        .ok_or("No access_token in response")?
        .to_string();

    let refresh_token = token_response["refresh_token"].as_str().map(String::from);

    let expires_in = token_response["expires_in"].as_i64();

    Ok((access_token, refresh_token, expires_in))
}

/// Save OAuth tokens to the MCP registry
async fn save_oauth_tokens(
    app: &AppHandle,
    server_id: &str,
    config: &ServerConfig,
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    scopes: &[String],
) -> Result<(), String> {
    let state = app.state::<Arc<AppState>>();
    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;

    // Calculate expiration timestamp
    let expires_at = expires_in.map(|secs| chrono::Utc::now().timestamp() + secs);

    // Get existing OAuth config and update with new tokens
    let updated_auth = match &config.auth {
        AuthMethod::OAuth {
            client_id,
            client_secret,
            authorization_url,
            token_url,
            ..
        } => AuthMethod::OAuth {
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
            authorization_url: authorization_url.clone(),
            token_url: token_url.clone(),
            scopes: scopes.to_vec(),
            access_token: Some(access_token),
            refresh_token,
            expires_at,
        },
        _ => return Err("Server is not configured for OAuth".to_string()),
    };

    let updated_config = ServerConfig {
        name: config.name.clone(),
        url: config.url.clone(),
        auth: updated_auth,
        use_well_known: config.use_well_known,
        auth_token: None,
        auto_connect: config.auto_connect,
        auto_retry: config.auto_retry,
    };

    registry.add_server(server_id.to_string(), updated_config);
    registry.save_config().map_err(|e| e.to_string())?;

    // Reconnect to apply the new token
    if registry.is_connected(server_id) {
        registry.disconnect(server_id).await.map_err(|e| e.to_string())?;
    }
    registry.connect(server_id).await.map_err(|e| e.to_string())?;

    log_message(&format!("Saved OAuth tokens for server: {}, reconnected with new token", server_id));
    Ok(())
}

/// Complete OAuth flow with authorization code
#[tauri::command]
pub async fn complete_mcp_oauth(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    code: String,
) -> Result<(), String> {
    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;

    let config = registry
        .config()
        .get_server(&server_id)
        .ok_or("Server not found")?
        .clone();

    match &config.auth {
        AuthMethod::OAuth {
            client_id,
            client_secret,
            token_url,
            authorization_url,
            scopes,
            ..
        } => {
            // Get token URL
            let tok_url = if let Some(url) = token_url {
                url.clone()
            } else if config.use_well_known {
                let well_known = fetch_well_known(&config.url).await?;
                well_known["token_endpoint"]
                    .as_str()
                    .ok_or("No token_endpoint in well-known config")?
                    .to_string()
            } else {
                return Err("OAuth requires token_url or use_well_known".to_string());
            };

            // Use same redirect_uri as in start_mcp_oauth
            let redirect_uri = "noema://oauth/callback";
            let http_client = reqwest::Client::new();

            let mut params = vec![
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", redirect_uri),
                ("client_id", client_id.as_str()),
            ];

            let client_secret_str;
            if let Some(secret) = client_secret {
                client_secret_str = secret.clone();
                params.push(("client_secret", &client_secret_str));
            }

            let resp = http_client
                .post(&tok_url)
                .form(&params)
                .send()
                .await
                .map_err(|e| format!("Token exchange failed: {}", e))?;

            if !resp.status().is_success() {
                let error_text = resp.text().await.unwrap_or_default();
                return Err(format!("Token exchange failed: {}", error_text));
            }

            let token_response: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse token response: {}", e))?;

            let access_token = token_response["access_token"]
                .as_str()
                .ok_or("No access_token in response")?
                .to_string();

            let refresh_token = token_response["refresh_token"].as_str().map(String::from);

            let expires_in = token_response["expires_in"].as_i64();
            let expires_at = expires_in.map(|exp| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
                    + exp
            });

            // Update the server config with tokens
            let updated_auth = AuthMethod::OAuth {
                client_id: client_id.clone(),
                client_secret: client_secret.clone(),
                authorization_url: authorization_url.clone(),
                token_url: Some(tok_url),
                scopes: scopes.clone(),
                access_token: Some(access_token),
                refresh_token,
                expires_at,
            };

            let updated_config = ServerConfig {
                name: config.name.clone(),
                url: config.url.clone(),
                auth: updated_auth,
                use_well_known: config.use_well_known,
                auth_token: None,
                auto_connect: config.auto_connect,
                auto_retry: config.auto_retry,
            };

            registry.add_server(server_id.clone(), updated_config);
            registry.save_config().map_err(|e| e.to_string())?;

            // Reconnect to apply the new token
            // First disconnect if connected
            if registry.is_connected(&server_id) {
                registry.disconnect(&server_id).await.map_err(|e| e.to_string())?;
            }
            // Then reconnect with the updated config (which now has the token)
            registry.connect(&server_id).await.map_err(|e| e.to_string())?;

            tracing::info!("OAuth complete for '{}', reconnected with new token", server_id);
            Ok(())
        }
        _ => Err("Server is not configured for OAuth".to_string()),
    }
}

/// Internal function to complete OAuth (shared by command and deep link handler)
pub async fn complete_oauth_internal(
    app: &AppHandle,
    server_id: &str,
    code: &str,
) -> Result<(), String> {
    let state = app.state::<Arc<AppState>>();
    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;

    let config = registry
        .config()
        .get_server(server_id)
        .ok_or("Server not found")?
        .clone();

    match &config.auth {
        AuthMethod::OAuth {
            client_id,
            client_secret,
            token_url,
            authorization_url,
            scopes,
            ..
        } => {
            // Get token URL
            let tok_url = if let Some(url) = token_url {
                url.clone()
            } else if config.use_well_known {
                let well_known = fetch_well_known(&config.url).await?;
                well_known["token_endpoint"]
                    .as_str()
                    .ok_or("No token_endpoint in well-known config")?
                    .to_string()
            } else {
                return Err("OAuth requires token_url or use_well_known".to_string());
            };

            let redirect_uri = "noema://oauth/callback";
            let http_client = reqwest::Client::new();

            let mut params = vec![
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", client_id.as_str()),
            ];

            let client_secret_str;
            if let Some(secret) = client_secret {
                client_secret_str = secret.clone();
                params.push(("client_secret", &client_secret_str));
            }

            let resp = http_client
                .post(&tok_url)
                .form(&params)
                .send()
                .await
                .map_err(|e| format!("Token exchange failed: {}", e))?;

            if !resp.status().is_success() {
                let error_text = resp.text().await.unwrap_or_default();
                return Err(format!("Token exchange failed: {}", error_text));
            }

            let token_response: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse token response: {}", e))?;

            let access_token = token_response["access_token"]
                .as_str()
                .ok_or("No access_token in response")?
                .to_string();

            let refresh_token = token_response["refresh_token"].as_str().map(String::from);

            let expires_in = token_response["expires_in"].as_i64();
            let expires_at = expires_in.map(|exp| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
                    + exp
            });

            // Update the server config with tokens
            let updated_auth = AuthMethod::OAuth {
                client_id: client_id.clone(),
                client_secret: client_secret.clone(),
                authorization_url: authorization_url.clone(),
                token_url: Some(tok_url),
                scopes: scopes.clone(),
                access_token: Some(access_token),
                refresh_token,
                expires_at,
            };

            let updated_config = ServerConfig {
                name: config.name.clone(),
                url: config.url.clone(),
                auth: updated_auth,
                use_well_known: config.use_well_known,
                auth_token: None,
                auto_connect: config.auto_connect,
                auto_retry: config.auto_retry,
            };

            registry.add_server(server_id.to_string(), updated_config);
            registry.save_config().map_err(|e| e.to_string())?;

            // Reconnect to apply the new token
            // First disconnect if connected
            if registry.is_connected(server_id) {
                registry.disconnect(server_id).await.map_err(|e| e.to_string())?;
            }
            // Then reconnect with the updated config (which now has the token)
            registry.connect(server_id).await.map_err(|e| e.to_string())?;

            tracing::info!("OAuth complete for '{}', reconnected with new token", server_id);
            Ok(())
        }
        _ => Err("Server is not configured for OAuth".to_string()),
    }
}

/// Update auto-connect and auto-retry settings for an MCP server
#[tauri::command]
pub async fn update_mcp_server_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    server_id: String,
    auto_connect: bool,
    auto_retry: bool,
) -> Result<(), String> {
    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;

    // Get existing config
    let config = registry
        .config()
        .get_server(&server_id)
        .ok_or("Server not found")?
        .clone();

    // Update config with new settings
    let updated_config = ServerConfig {
        name: config.name,
        url: config.url.clone(),
        auth: config.auth,
        use_well_known: config.use_well_known,
        auth_token: config.auth_token,
        auto_connect,
        auto_retry,
    };

    registry.add_server(server_id.clone(), updated_config.clone());
    registry.save_config().map_err(|e| e.to_string())?;

    // If auto_retry was disabled, cancel any active retry
    if !auto_retry {
        registry.cancel_retry(&server_id);
    }

    // If auto_retry was enabled and server is not connected, start retry
    if auto_retry && !registry.is_connected(&server_id) && !registry.is_retry_active(&server_id) {
        // Create callback for status updates
        let app_handle = app.clone();
        let cb: Option<Box<dyn Fn(&str, &ServerStatus) + Send + Sync>> =
            Some(Box::new(move |server_id: &str, status: &ServerStatus| {
                let status_str = match status {
                    ServerStatus::Disconnected => "disconnected".to_string(),
                    ServerStatus::Connected => "connected".to_string(),
                    ServerStatus::Retrying { attempt } => format!("retrying:{}", attempt),
                    ServerStatus::RetryStopped { last_error } => format!("stopped:{}", last_error),
                };

                log_message(&format!("MCP server '{}' status: {}", server_id, status_str));

                let _ = app_handle.emit(
                    "mcp_server_status",
                    serde_json::json!({
                        "server_id": server_id,
                        "status": status_str,
                    }),
                );
            }));

        let token = spawn_retry_task(
            Arc::clone(&mcp_registry),
            server_id.clone(),
            updated_config,
            cb,
        );
        registry.set_retry_token(&server_id, token);
    }

    Ok(())
}

/// Stop retry attempts for an MCP server
#[tauri::command]
pub async fn stop_mcp_retry(state: State<'_, Arc<AppState>>, server_id: String) -> Result<(), String> {
    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;

    registry.cancel_retry(&server_id);
    registry.set_status(
        &server_id,
        ServerStatus::RetryStopped {
            last_error: "Manually stopped".to_string(),
        },
    );

    Ok(())
}

/// Start retry attempts for an MCP server
#[tauri::command]
pub async fn start_mcp_retry(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<(), String> {
    let mcp_registry = state.get_mcp_registry()?;
    let mut registry = mcp_registry.lock().await;

    // Check if already connected or retry in progress
    if registry.is_connected(&server_id) {
        return Err("Server is already connected".to_string());
    }
    if registry.is_retry_active(&server_id) {
        return Err("Retry is already in progress".to_string());
    }

    let config = registry
        .config()
        .get_server(&server_id)
        .ok_or("Server not found")?
        .clone();

    // Create callback for status updates
    let app_handle = app.clone();
    let cb: Option<Box<dyn Fn(&str, &ServerStatus) + Send + Sync>> =
        Some(Box::new(move |server_id: &str, status: &ServerStatus| {
            let status_str = match status {
                ServerStatus::Disconnected => "disconnected".to_string(),
                ServerStatus::Connected => "connected".to_string(),
                ServerStatus::Retrying { attempt } => format!("retrying:{}", attempt),
                ServerStatus::RetryStopped { last_error } => format!("stopped:{}", last_error),
            };

            log_message(&format!("MCP server '{}' status: {}", server_id, status_str));

            let _ = app_handle.emit(
                "mcp_server_status",
                serde_json::json!({
                    "server_id": server_id,
                    "status": status_str,
                }),
            );
        }));

    let token = spawn_retry_task(Arc::clone(&mcp_registry), server_id.clone(), config, cb);
    registry.set_retry_token(&server_id, token);

    Ok(())
}

/// Handle incoming deep link URLs (e.g., noema://oauth/callback?code=...&state=...)
pub async fn handle_deep_link(app: &AppHandle, urls: Vec<url::Url>) {
    for url in urls {
        log_message(&format!("Deep link received: {}", url));

        // Check if this is an OAuth callback
        // Note: In noema://oauth/callback, "oauth" is the host and "/callback" is the path
        let is_oauth_callback = url.scheme() == "noema"
            && url.host_str() == Some("oauth")
            && url.path() == "/callback";

        if is_oauth_callback {
            // Extract the code and state from query params
            let code = url
                .query_pairs()
                .find(|(key, _)| key == "code")
                .map(|(_, value)| value.to_string());

            let state_param = url
                .query_pairs()
                .find(|(key, _)| key == "state")
                .map(|(_, value)| value.to_string());

            if let (Some(auth_code), Some(oauth_state)) = (code.as_ref(), state_param.as_ref()) {
                let app_state = app.state::<AppState>();

                // Look up server ID from state parameter
                let server_id = {
                    let mut pending_states = app_state.pending_oauth_states.lock().await;
                    let server_id = pending_states.remove(oauth_state);

                    // Update persisted state
                    if server_id.is_some() {
                        if let Err(e) = save_pending_oauth_states(&pending_states) {
                            log_message(&format!(
                                "Warning: Failed to update persisted OAuth state: {}",
                                e
                            ));
                        }
                    }

                    server_id
                };

                log_message(&format!("Found server_id for state: {:?}", server_id));

                if let Some(server_id) = server_id {
                    // Complete OAuth flow
                    match complete_oauth_internal(app, &server_id, auth_code).await {
                        Ok(()) => {
                            log_message(&format!(
                                "OAuth completed successfully for server: {}",
                                server_id
                            ));
                            // Emit success event to frontend
                            app.emit("oauth_complete", &server_id).ok();
                        }
                        Err(e) => {
                            log_message(&format!("OAuth error: {}", e));
                            // Emit error event to frontend
                            app.emit("oauth_error", &e).ok();
                        }
                    }
                } else {
                    let err = format!("No pending OAuth flow found for state: {}", oauth_state);
                    log_message(&err);
                    app.emit("oauth_error", &err).ok();
                }
            } else {
                // Missing code or state - log but don't emit error (may be duplicate/incomplete callback)
                log_message(&format!(
                    "Incomplete OAuth callback - code: {:?}, state: {:?}",
                    code.is_some(),
                    state_param.is_some()
                ));
            }
        }
    }
}
