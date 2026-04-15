//! Per-user MCP OAuth routes.
//!
//! Generic OAuth flow that works for any MCP server (Google, GitHub, Notion, etc.).
//! OAuth config comes from the MCP server's config in mcp.toml.
//! Tokens stored transiently in-memory via TransientTokenStore.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;

use simply_core::storage::ids::UserId;

use crate::oauth::providers::resolve_server_auth;
use crate::token_store::{McpUserToken, TransientTokenStore};

/// Shared state for MCP auth routes.
#[derive(Clone)]
pub struct McpAuthState {
    pub token_store: Arc<TransientTokenStore>,
    pub public_url: String,
}

#[derive(Deserialize)]
pub struct AuthInitQuery {
    pub user_id: String,
}

#[derive(Deserialize)]
pub struct AuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Encoded in the OAuth `state` parameter.
#[derive(serde::Serialize, serde::Deserialize)]
struct OAuthState {
    user_id: String,
    server_id: String,
}

/// `GET /auth/mcp/{server_id}?user_id=...`
///
/// Initiates the OAuth flow by redirecting the user to the provider's consent screen.
pub async fn auth_initiate(
    State(state): State<McpAuthState>,
    Path(server_id): Path<String>,
    Query(query): Query<AuthInitQuery>,
) -> Response {
    let mcp_config = crate::mcp_config::load_mcp_config();
    let server = match mcp_config.get_server(&server_id) {
        Some(s) => s,
        None => return Html(format!("Unknown MCP server: {server_id}")).into_response(),
    };

    let oauth = match resolve_server_auth(server) {
        Some(o) => o,
        None => return Html(format!(
            "OAuth not configured for server '{server_id}'. Set oauth_provider + client_id in mcp.toml."
        )).into_response(),
    };

    let client_id = oauth.client_id;
    let authorization_url = oauth.authorization_url;
    let scopes = oauth.scopes;

    let oauth_state = OAuthState {
        user_id: query.user_id,
        server_id,
    };
    let state_json = serde_json::to_string(&oauth_state).unwrap_or_default();
    let state_encoded = urlencoding::encode(&state_json);

    let redirect_uri = format!("{}/auth/mcp/callback", state.public_url);
    let scope = scopes.join(" ");

    let url = format!(
        "{authorization_url}?client_id={client_id}&redirect_uri={redirect_uri}&response_type=code&scope={scope}&state={state_encoded}&access_type=offline&prompt=consent",
        redirect_uri = urlencoding::encode(&redirect_uri),
        scope = urlencoding::encode(&scope),
    );

    Redirect::temporary(&url).into_response()
}

/// `GET /auth/mcp/callback?code=...&state=...`
///
/// OAuth callback — exchanges code for token, stores in TransientTokenStore.
pub async fn auth_callback(
    State(state): State<McpAuthState>,
    Query(query): Query<AuthCallbackQuery>,
) -> Response {
    if let Some(error) = query.error {
        return auth_error_page(&format!("OAuth error: {error}"));
    }

    let (code, state_json) = match (query.code, query.state) {
        (Some(c), Some(s)) => (c, s),
        _ => return auth_error_page("Missing code or state parameter"),
    };

    let state_decoded = match urlencoding::decode(&state_json) {
        Ok(s) => s.to_string(),
        Err(_) => state_json,
    };

    let oauth_state: OAuthState = match serde_json::from_str(&state_decoded) {
        Ok(s) => s,
        Err(_) => return auth_error_page("Invalid state parameter"),
    };

    // Look up OAuth config for token exchange
    let mcp_config = crate::mcp_config::load_mcp_config();
    let server = match mcp_config.get_server(&oauth_state.server_id) {
        Some(s) => s,
        None => return auth_error_page(&format!("Unknown server: {}", oauth_state.server_id)),
    };
    let oauth = match resolve_server_auth(server) {
        Some(o) => o,
        None => return auth_error_page(&format!("OAuth not configured for server: {}", oauth_state.server_id)),
    };

    let client_id = oauth.client_id;
    let client_secret = oauth.client_secret;
    let token_url = oauth.token_url;

    // Exchange code for token
    let redirect_uri = format!("{}/auth/mcp/callback", state.public_url);
    let client = reqwest::Client::new();

    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", &redirect_uri),
        ("client_id", &client_id),
    ];
    let secret_ref;
    if let Some(ref secret) = client_secret {
        secret_ref = secret.clone();
        params.push(("client_secret", &secret_ref));
    }

    let resp = match client.post(&token_url).form(&params).send().await {
        Ok(r) => r,
        Err(e) => return auth_error_page(&format!("Token exchange failed: {e}")),
    };

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return auth_error_page(&format!("Token exchange returned {}: {}", body.len(), body));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        expires_in: Option<u64>,
    }

    let token_resp: TokenResponse = match resp.json().await {
        Ok(t) => t,
        Err(e) => return auth_error_page(&format!("Failed to parse token response: {e}")),
    };

    let expires_at = token_resp
        .expires_in
        .map(|secs| Instant::now() + Duration::from_secs(secs));

    let user_id = UserId::from_string(&oauth_state.user_id);
    state.token_store.store(
        &user_id,
        &oauth_state.server_id,
        McpUserToken {
            access_token: token_resp.access_token,
            expires_at,
            identity: None,
        },
    );

    tracing::info!(
        user_id = %oauth_state.user_id,
        server_id = %oauth_state.server_id,
        "OAuth token stored"
    );

    auth_success_page(&oauth_state.server_id)
}

fn auth_success_page(server_id: &str) -> Response {
    Html(format!(
        r#"<!DOCTYPE html><html><head><title>Connected</title></head>
        <body style="font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#1a1a1a;color:#fff">
        <div style="text-align:center">
        <h1 style="color:#14b8a6">Connected to {server_id}</h1>
        <p>You can close this window and return to Discord.</p>
        </div></body></html>"#
    ))
    .into_response()
}

fn auth_error_page(message: &str) -> Response {
    Html(format!(
        r#"<!DOCTYPE html><html><head><title>Auth Failed</title></head>
        <body style="font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#1a1a1a;color:#fff">
        <div style="text-align:center">
        <h1 style="color:#ef4444">Authentication Failed</h1>
        <p>{message}</p>
        </div></body></html>"#
    ))
    .into_response()
}
