//! MCP (Model Context Protocol) server commands

use noema_core::{AuthMethod, ServerConfig};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::logging::log_message;
use crate::state::{save_pending_oauth_states, AppState};
use crate::types::{AddMcpServerRequest, McpServerInfo, McpToolInfo};

/// List all configured MCP servers
#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerInfo>, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
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

        servers.push(McpServerInfo {
            id: id.to_string(),
            name: config.name.clone(),
            url: config.url.clone(),
            auth_type: auth_type.to_string(),
            is_connected,
            needs_oauth_login: config.auth.needs_oauth_login(),
            tool_count,
        });
    }

    Ok(servers)
}

/// Add a new MCP server configuration
/// If auth_type is not specified or is "auto", probe .well-known to detect OAuth
#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, AppState>,
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
                scopes: request.scopes,
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
    };

    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
    let mut registry = mcp_registry.lock().await;
    registry.add_server(request.id, config);
    registry.save_config().map_err(|e| e.to_string())?;

    Ok(())
}

/// Remove an MCP server configuration
#[tauri::command]
pub async fn remove_mcp_server(state: State<'_, AppState>, server_id: String) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
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
pub async fn connect_mcp_server(state: State<'_, AppState>, server_id: String) -> Result<usize, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
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
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
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
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpToolInfo>, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
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
pub async fn test_mcp_server(state: State<'_, AppState>, server_id: String) -> Result<usize, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
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
#[tauri::command]
pub async fn start_mcp_oauth(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<String, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
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
            authorization_url,
            scopes,
            ..
        } => {
            let redirect_uri = "noema://oauth/callback";

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

            // Check if we need to register the client dynamically.
            // We always re-register if client_id is "noema", empty, or if there's no access_token yet
            // (which means a previous registration may have used a different redirect_uri).
            let needs_registration = client_id == "noema" || client_id.is_empty();

            let (final_client_id, _final_client_secret) = if needs_registration {
                // Try dynamic client registration
                if let Some(ref wk) = well_known {
                    if let Some(reg_endpoint) = wk["registration_endpoint"].as_str() {
                        let (new_id, new_secret) =
                            register_oauth_client(reg_endpoint, redirect_uri).await?;

                        // Update config with new client credentials
                        let updated_auth = AuthMethod::OAuth {
                            client_id: new_id.clone(),
                            client_secret: new_secret.clone(),
                            authorization_url: Some(auth_url.clone()),
                            token_url: wk["token_endpoint"].as_str().map(String::from),
                            scopes: scopes.clone(),
                            access_token: None,
                            refresh_token: None,
                            expires_at: None,
                        };

                        let updated_config = ServerConfig {
                            name: config.name.clone(),
                            url: config.url.clone(),
                            auth: updated_auth,
                            use_well_known: config.use_well_known,
                            auth_token: None,
                        };

                        registry.add_server(server_id.clone(), updated_config);
                        registry.save_config().map_err(|e| e.to_string())?;

                        (new_id, new_secret)
                    } else {
                        return Err("Server requires client registration but no registration_endpoint found. Please configure client_id manually.".to_string());
                    }
                } else {
                    return Err("Cannot register client without .well-known discovery".to_string());
                }
            } else {
                (client_id.clone(), client_secret.clone())
            };

            // Build authorization URL with state parameter that maps to server_id
            let state_param = uuid::Uuid::new_v4().to_string();

            // Store the state -> server_id mapping in memory and persist to disk
            {
                let mut pending_states = state.pending_oauth_states.lock().await;
                pending_states.insert(state_param.clone(), server_id.clone());
                // Persist to disk so it survives app restart
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
                .append_pair("redirect_uri", redirect_uri)
                .append_pair("state", &state_param)
                .append_pair("scope", &scope_str);

            Ok(url.to_string())
        }
        _ => Err("Server is not configured for OAuth".to_string()),
    }
}

/// Complete OAuth flow with authorization code
#[tauri::command]
pub async fn complete_mcp_oauth(
    state: State<'_, AppState>,
    server_id: String,
    code: String,
) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
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
            };

            registry.add_server(server_id, updated_config);
            registry.save_config().map_err(|e| e.to_string())?;

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
    let state = app.state::<AppState>();
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let mcp_registry = engine.get_mcp_registry();
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
            };

            registry.add_server(server_id.to_string(), updated_config);
            registry.save_config().map_err(|e| e.to_string())?;

            Ok(())
        }
        _ => Err("Server is not configured for OAuth".to_string()),
    }
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
