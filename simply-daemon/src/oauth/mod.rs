//! OAuth support for MCP servers.
//!
//! `OAuthService` encapsulates the full OAuth lifecycle — pending state tracking,
//! callback server, well-known discovery, token exchange, and config updates.
//! Both `EmbeddedDaemon` and the future standalone daemon share this.

pub mod callback;
pub mod providers;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use simply_core::McpRegistry;

use crate::api::OAuthFlowInfo;
use callback::CallbackTracker;

// ---------------------------------------------------------------------------
// OAuthService
// ---------------------------------------------------------------------------

/// Manages OAuth flows for MCP servers.
///
/// Uses the daemon's main HTTP server for OAuth callbacks (no separate port).
/// Concurrent flows are multiplexed by `state` parameter.
pub struct OAuthService {
    mcp_registry: Arc<Mutex<McpRegistry>>,
    tracker: Arc<CallbackTracker>,
    /// state -> server_id for flows initiated via deep link (not callback server)
    pending_states: Mutex<HashMap<String, String>>,
}

impl OAuthService {
    /// Create an OAuthService. `public_url` is the daemon's externally-reachable
    /// base URL (from settings or auto-derived from port).
    pub fn new(
        mcp_registry: Arc<Mutex<McpRegistry>>,
        public_url: String,
    ) -> Self {
        Self {
            mcp_registry,
            tracker: Arc::new(CallbackTracker::new(public_url)),
            pending_states: Mutex::new(HashMap::new()),
        }
    }

    /// The callback tracker — share with the axum route handler.
    pub fn tracker(&self) -> Arc<CallbackTracker> {
        Arc::clone(&self.tracker)
    }

    /// The stable redirect URI (on the daemon's main port).
    pub fn redirect_uri(&self) -> String {
        self.tracker.redirect_uri()
    }

    /// Start an OAuth flow: register on the tracker, build the
    /// authorization URL, spawn a background task to complete on callback.
    pub async fn start_flow(&self, server_id: &str) -> anyhow::Result<OAuthFlowInfo> {
        let config = {
            let registry = self.mcp_registry.lock().await;
            registry
                .config()
                .get_server(server_id)
                .ok_or_else(|| anyhow::anyhow!("server not found: {server_id}"))?
                .clone()
        };

        let (client_id, authorization_url, scopes) = match &config.auth {
            simply_core::AuthMethod::OAuth {
                client_id,
                authorization_url,
                scopes,
                ..
            } => (client_id.clone(), authorization_url.clone(), scopes.clone()),
            _ => anyhow::bail!("server is not configured for OAuth: {server_id}"),
        };

        if client_id == "simply" || client_id.is_empty() {
            anyhow::bail!("Please configure your OAuth Client ID in the server settings first.");
        }

        let redirect_uri = self.tracker.redirect_uri();

        // Fetch .well-known if needed
        let well_known = if config.use_well_known {
            Some(fetch_well_known(&config.url).await?)
        } else {
            None
        };

        // Resolve authorization URL
        let auth_url = if let Some(url) = authorization_url {
            url
        } else if let Some(ref wk) = well_known {
            wk["authorization_endpoint"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("No authorization_endpoint in .well-known"))?
                .to_string()
        } else {
            anyhow::bail!("OAuth requires authorization_url or use_well_known");
        };

        // Generate state, register with callback server, and track server_id
        let state_param = uuid::Uuid::new_v4().to_string();
        let callback_rx = self.tracker.register(&state_param).await;
        self.pending_states
            .lock()
            .await
            .insert(state_param.clone(), server_id.to_string());

        // Resolve scopes: server config > .well-known > fallback to openid
        let scope_str = if !scopes.is_empty() {
            scopes.join(" ")
        } else if let Some(ref wk) = well_known {
            wk["scopes_supported"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" "))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "openid".to_string())
        } else {
            "openid".to_string()
        };

        let mut url = url::Url::parse(&auth_url)?;
        url.query_pairs_mut()
            .append_pair("client_id", &client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("state", &state_param)
            .append_pair("scope", &scope_str)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent");

        let auth_url_str = url.to_string();

        // Spawn background task to wait for the callback and complete the flow
        let registry = Arc::clone(&self.mcp_registry);
        let server_id_owned = server_id.to_string();
        let state_clone = state_param.clone();
        let server_url = config.url.clone();
        let use_well_known = config.use_well_known;

        tokio::spawn(async move {
            let result = callback_rx.await;
            match result {
                Ok(Ok((code, received_state))) => {
                    if received_state != state_clone {
                        tracing::error!("OAuth state mismatch");
                        return;
                    }
                    if let Err(e) = complete_token_exchange(
                        &registry,
                        &server_id_owned,
                        &code,
                        &redirect_uri,
                        &server_url,
                        use_well_known,
                    )
                    .await
                    {
                        tracing::error!(error = %e, "OAuth token exchange failed");
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "OAuth callback returned error");
                }
                Err(_) => {
                    tracing::error!("OAuth callback cancelled");
                }
            }
        });

        Ok(OAuthFlowInfo {
            auth_url: auth_url_str,
            state: state_param,
        })
    }

    /// Complete OAuth with state verification (e.g., deep link callback).
    pub async fn complete_with_state(
        &self,
        server_id: &str,
        code: &str,
        state: &str,
    ) -> anyhow::Result<()> {
        let expected_server = self
            .pending_states
            .lock()
            .await
            .remove(state)
            .ok_or_else(|| anyhow::anyhow!("unknown or expired OAuth state"))?;

        if expected_server != server_id {
            anyhow::bail!("OAuth state does not match server_id");
        }

        // Cancel the callback server registration since we're completing directly
        self.tracker.cancel(state).await;

        let (server_url, use_well_known) = self.get_server_url(server_id).await?;

        complete_token_exchange(
            &self.mcp_registry,
            server_id,
            code,
            &self.tracker.redirect_uri(),
            &server_url,
            use_well_known,
        )
        .await
    }

    /// Complete OAuth with just a code (manual entry, no state verification).
    pub async fn complete_with_code(
        &self,
        server_id: &str,
        code: &str,
    ) -> anyhow::Result<()> {
        let (server_url, use_well_known) = self.get_server_url(server_id).await?;

        complete_token_exchange(
            &self.mcp_registry,
            server_id,
            code,
            &self.tracker.redirect_uri(),
            &server_url,
            use_well_known,
        )
        .await
    }

    /// Look up server_id for a pending OAuth state without consuming it.
    pub async fn resolve_state(&self, state: &str) -> Option<String> {
        self.pending_states.lock().await.get(state).cloned()
    }

    async fn get_server_url(&self, server_id: &str) -> anyhow::Result<(String, bool)> {
        let registry = self.mcp_registry.lock().await;
        let config = registry
            .config()
            .get_server(server_id)
            .ok_or_else(|| anyhow::anyhow!("server not found: {server_id}"))?;
        Ok((config.url.clone(), config.use_well_known))
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Fetch `.well-known/oauth-authorization-server` from an MCP server URL.
pub async fn fetch_well_known(base_url: &str) -> anyhow::Result<serde_json::Value> {
    let base = url::Url::parse(base_url)?;
    let well_known_url = base.join("/.well-known/oauth-authorization-server")?;

    let client = reqwest::Client::new();
    let resp = client.get(well_known_url.as_str()).send().await?;
    let json = resp.json().await?;
    Ok(json)
}

/// Exchange authorization code for tokens, save to registry, and reconnect.
async fn complete_token_exchange(
    mcp_registry: &Arc<Mutex<McpRegistry>>,
    server_id: &str,
    code: &str,
    redirect_uri: &str,
    server_url: &str,
    use_well_known: bool,
) -> anyhow::Result<()> {
    let config = {
        let registry = mcp_registry.lock().await;
        registry
            .config()
            .get_server(server_id)
            .ok_or_else(|| anyhow::anyhow!("server not found: {server_id}"))?
            .clone()
    };

    let (client_id, client_secret, token_url, authorization_url, scopes) = match &config.auth {
        simply_core::AuthMethod::OAuth {
            client_id,
            client_secret,
            token_url,
            authorization_url,
            scopes,
            ..
        } => (
            client_id.clone(),
            client_secret.clone(),
            token_url.clone(),
            authorization_url.clone(),
            scopes.clone(),
        ),
        _ => anyhow::bail!("server is not configured for OAuth"),
    };

    // Resolve token URL
    let tok_url = if let Some(url) = token_url {
        url
    } else if use_well_known {
        let well_known = fetch_well_known(server_url).await?;
        well_known["token_endpoint"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No token_endpoint in .well-known"))?
            .to_string()
    } else {
        anyhow::bail!("OAuth requires token_url or use_well_known");
    };

    // Exchange code for tokens
    let tokens = exchange_code_for_tokens(
        &tok_url,
        code,
        redirect_uri,
        &client_id,
        client_secret.as_deref(),
    )
    .await?;

    let expires_at = tokens
        .expires_in
        .map(|secs| chrono::Utc::now().timestamp() + secs);

    // Update server config with new tokens
    let updated_auth = simply_core::AuthMethod::OAuth {
        client_id,
        client_secret,
        authorization_url,
        token_url: Some(tok_url),
        scopes,
        access_token: Some(tokens.access_token),
        refresh_token: tokens.refresh_token,
        expires_at,
    };

    let updated_config = simply_core::ServerConfig {
        auth: updated_auth,
        ..config
    };

    let mut registry = mcp_registry.lock().await;
    registry.add_server(server_id.to_string(), updated_config);
    crate::mcp::config_io::save_mcp_config(registry.config())?;

    // Reconnect with new token
    if registry.is_connected(server_id) {
        registry.disconnect(server_id).await?;
    }
    registry.connect(server_id).await?;

    tracing::info!(server_id, "OAuth complete, reconnected with new token");
    Ok(())
}

/// Exchange an authorization code for tokens at the given token endpoint.
async fn exchange_code_for_tokens(
    token_url: &str,
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: Option<&str>,
) -> anyhow::Result<TokenResponse> {
    let http_client = reqwest::Client::new();

    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
    ];

    let secret_owned;
    if let Some(secret) = client_secret {
        secret_owned = secret.to_string();
        params.push(("client_secret", &secret_owned));
    }

    let resp = http_client.post(token_url).form(&params).send().await?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Token exchange failed: {}", error_text);
    }

    let token_response: serde_json::Value = resp.json().await?;

    let access_token = token_response["access_token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No access_token in response"))?
        .to_string();

    let refresh_token = token_response["refresh_token"].as_str().map(String::from);
    let expires_in = token_response["expires_in"].as_i64();

    Ok(TokenResponse {
        access_token,
        refresh_token,
        expires_in,
    })
}

struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}
