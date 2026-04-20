//! Per-user OAuth routes.
//!
//! `/auth/mcp/{id}?user_id=...` — initiates an OAuth flow for a user.
//! `{id}` is either:
//!   - An MCP server ID (from mcp.toml) — scopes come from the server config
//!   - An OAuth provider ID (from oauth_providers.toml) — scopes come from the
//!     in-memory union of all skill OAuthRequirement declarations for that provider
//!
//! Tokens are stored per-user in `TransientTokenStore` keyed by the resolved
//! provider_id (so one login covers all consumers of that provider).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;

use simply_core::storage::traits::UserStore;

use crate::mcp::auth::{DaemonMcpConfig, ServerAuth};
use crate::oauth::callback::CallbackTracker;
use crate::oauth::providers::{lookup_provider, ResolvedOAuth};
use crate::token_store::{McpUserToken, TransientTokenStore};

/// Shared state for MCP auth routes.
#[derive(Clone)]
pub struct McpAuthState {
    pub token_store: Arc<TransientTokenStore>,
    pub user_store: Arc<dyn UserStore>,
    pub public_url: String,
    /// OAuth callback tracker — bridges OAuthService.start_flow() to this route.
    pub oauth_tracker: Option<Arc<CallbackTracker>>,
    /// MCP server configs (mcp.toml).
    pub daemon_mcp_config: Arc<tokio::sync::Mutex<DaemonMcpConfig>>,
    /// In-memory union of scopes declared by skills per provider.
    pub skill_scopes: Arc<tokio::sync::Mutex<HashMap<String, HashSet<String>>>>,
}

#[derive(Deserialize)]
pub struct AuthInitQuery {
    /// External identity that initiated the flow (e.g. "discord:123456").
    /// The callback will create (or reuse) a user with this external_id linked
    /// and store the OAuth token under that user. No user exists yet — the
    /// email from OAuth is the canonical identity.
    pub external_id: String,
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
    /// External identity that initiated the flow — linked to the user in the callback.
    external_id: String,
    /// The provider_id tokens will be stored under (shared across consumers).
    provider_id: String,
}

/// Resolve `{id}` (server_id or provider_id) into (provider_id, ResolvedOAuth).
fn resolve_for_auth(
    id: &str,
    daemon_cfg: &DaemonMcpConfig,
    skill_scopes: &HashMap<String, HashSet<String>>,
) -> Option<(String, ResolvedOAuth)> {
    // 1. MCP server — scopes come from ServerAuth::OAuth.scopes
    if let Some(server) = daemon_cfg.get_server(id) {
        if let ServerAuth::OAuth { provider_id, scopes } = &server.auth {
            let provider = lookup_provider(provider_id)?;
            return Some((provider_id.clone(), ResolvedOAuth {
                client_id: provider.client_id,
                client_secret: provider.client_secret,
                authorization_url: provider.authorization_url,
                token_url: provider.token_url,
                userinfo_url: provider.userinfo_url,
                scopes: scopes.clone(),
            }));
        }
    }
    // 2. Direct provider (skills) — scopes from in-memory union
    if let Some(provider) = lookup_provider(id) {
        let scopes: Vec<String> = skill_scopes.get(id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        return Some((id.to_string(), ResolvedOAuth {
            client_id: provider.client_id,
            client_secret: provider.client_secret,
            authorization_url: provider.authorization_url,
            token_url: provider.token_url,
            userinfo_url: provider.userinfo_url,
            scopes,
        }));
    }
    None
}

/// `GET /auth/mcp/{id}?user_id=...` — initiate per-user OAuth.
pub async fn auth_initiate(
    State(state): State<McpAuthState>,
    Path(id): Path<String>,
    Query(query): Query<AuthInitQuery>,
) -> Response {
    let daemon_cfg = state.daemon_mcp_config.lock().await;
    let skill_scopes = state.skill_scopes.lock().await;

    let (provider_id, oauth) = match resolve_for_auth(&id, &daemon_cfg, &skill_scopes) {
        Some(r) => r,
        None => return Html(format!("Unknown OAuth provider or MCP server: {id}")).into_response(),
    };
    drop(daemon_cfg);
    drop(skill_scopes);

    if oauth.client_id.is_empty() {
        return Html(format!(
            "OAuth client_id not configured for provider '{provider_id}'. Set it in oauth_providers.toml or via settings."
        )).into_response();
    }

    let redirect_uri = format!("{}/auth/mcp/callback", state.public_url);
    let scope = oauth.scopes.join(" ");

    let oauth_state = OAuthState {
        external_id: query.external_id,
        provider_id,
    };
    let state_json = serde_json::to_string(&oauth_state).unwrap_or_default();
    let state_encoded = urlencoding::encode(&state_json);

    let url = format!(
        "{auth_url}?client_id={client_id}&redirect_uri={redirect_uri}&response_type=code&scope={scope}&state={state_encoded}&access_type=offline&prompt=consent",
        auth_url = oauth.authorization_url,
        client_id = oauth.client_id,
        redirect_uri = urlencoding::encode(&redirect_uri),
        scope = urlencoding::encode(&scope),
    );

    Redirect::temporary(&url).into_response()
}

/// `GET /auth/mcp/callback?code=...&state=...`
pub async fn auth_callback(
    State(state): State<McpAuthState>,
    Query(query): Query<AuthCallbackQuery>,
) -> Response {
    if let Some(ref error) = query.error {
        if let (Some(ref tracker), Some(ref s)) = (&state.oauth_tracker, &query.state) {
            tracker.dispatch_error(s, error).await;
        }
        return auth_error_page(&format!("OAuth error: {error}"));
    }

    let (code, state_json) = match (&query.code, &query.state) {
        (Some(c), Some(s)) => (c.clone(), s.clone()),
        _ => return auth_error_page("Missing code or state parameter"),
    };

    // Try the OAuthService tracker first (for server-level flows from admin UI)
    if let Some(ref tracker) = state.oauth_tracker {
        if tracker.dispatch(&state_json, &code).await {
            return auth_success_page("MCP Server");
        }
    }

    // Per-user token exchange
    let state_decoded = urlencoding::decode(&state_json)
        .map(|s| s.to_string())
        .unwrap_or(state_json);

    let oauth_state: OAuthState = match serde_json::from_str(&state_decoded) {
        Ok(s) => s,
        Err(_) => return auth_error_page("Invalid state parameter"),
    };

    let provider = match lookup_provider(&oauth_state.provider_id) {
        Some(p) => p,
        None => return auth_error_page(&format!("Unknown OAuth provider: {}", oauth_state.provider_id)),
    };

    let redirect_uri = format!("{}/auth/mcp/callback", state.public_url);
    let client = reqwest::Client::new();

    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", &redirect_uri),
        ("client_id", &provider.client_id),
    ];
    let secret_ref;
    if let Some(ref secret) = provider.client_secret {
        secret_ref = secret.clone();
        params.push(("client_secret", &secret_ref));
    }

    let resp = match client.post(&provider.token_url).form(&params).send().await {
        Ok(r) => r,
        Err(e) => return auth_error_page(&format!("Token exchange failed: {e}")),
    };

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return auth_error_page(&format!("Token exchange error: {body}"));
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

    // Resolve the user: email is canonical. Get-or-create a user by email and
    // link the external_id to it. If the external_id was previously linked
    // to a different (anonymous) user, this overwrites the mapping — the old
    // stub user is orphaned.
    let Some(ref userinfo_url) = provider.userinfo_url else {
        return auth_error_page(&format!(
            "Provider '{}' has no userinfo_url configured — cannot resolve the user's email. \
             Set userinfo_url in oauth_providers.toml or via the admin UI.",
            oauth_state.provider_id,
        ));
    };
    let email = match fetch_userinfo_email(userinfo_url, &token_resp.access_token).await {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, userinfo_url, "userinfo fetch failed");
            return auth_error_page(&format!(
                "Failed to fetch user identity from {userinfo_url}: {e}"
            ));
        }
    };

    let stored_user = match state.user_store.get_or_create_user_by_email(&email).await {
        Ok(u) => u,
        Err(e) => return auth_error_page(&format!("Failed to resolve/create user: {e}")),
    };
    let user_id = stored_user.id.clone();

    if let Err(e) = state.user_store.link_external_id(&user_id, &oauth_state.external_id).await {
        tracing::warn!(error = %e, "failed to link external_id to user");
    }

    state.token_store.store(
        &user_id,
        &oauth_state.provider_id,
        McpUserToken {
            access_token: token_resp.access_token,
            expires_at,
            identity: Some(email.clone()),
        },
    );

    tracing::info!(
        user_id = %user_id,
        external_id = %oauth_state.external_id,
        email = %email,
        provider_id = %oauth_state.provider_id,
        "OAuth token stored, external_id linked"
    );

    auth_success_page(&oauth_state.provider_id)
}

async fn fetch_userinfo_email(userinfo_url: &str, access_token: &str) -> anyhow::Result<String> {
    let resp = reqwest::Client::new()
        .get(userinfo_url)
        .bearer_auth(access_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("userinfo request failed: {}", resp.status());
    }

    let json: serde_json::Value = resp.json().await?;
    json.get("email")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("no email in userinfo response"))
}

fn auth_success_page(provider_id: &str) -> Response {
    Html(format!(
        r#"<!DOCTYPE html><html><head><title>Connected</title></head>
        <body style="font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#1a1a1a;color:#fff">
        <div style="text-align:center">
        <h1 style="color:#14b8a6">Connected to {provider_id}</h1>
        <p>This window will close automatically…</p>
        </div>
        <script>setTimeout(function(){{ window.close(); }}, 1500);</script>
        </body></html>"#
    )).into_response()
}

fn auth_error_page(message: &str) -> Response {
    Html(format!(
        r#"<!DOCTYPE html><html><head><title>Auth Failed</title></head>
        <body style="font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#1a1a1a;color:#fff">
        <div style="text-align:center">
        <h1 style="color:#ef4444">Authentication Failed</h1>
        <p>{message}</p>
        </div></body></html>"#
    )).into_response()
}
