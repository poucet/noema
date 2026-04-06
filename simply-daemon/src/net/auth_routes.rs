//! Auth routes — Google OAuth login and user token issuance.
//!
//! Routes:
//! - GET  /auth/login?redirect=<url>  — start Google OAuth flow
//! - GET  /auth/callback              — OAuth callback, issues user token
//! - POST /auth/token                 — exchange user credentials for token (future)

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::http::StatusCode;

use simply_core::storage::traits::UserStore;

/// Shared state for auth routes.
#[derive(Clone)]
pub struct AuthState {
    pub user_store: Arc<dyn UserStore>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub admin_email: Option<String>,
    pub daemon_port: u16,
}

#[derive(serde::Deserialize)]
pub struct LoginQuery {
    discord_id: Option<String>,
    redirect: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// GET /auth/login — redirect to Google OAuth
pub async fn auth_login(
    State(state): State<AuthState>,
    Query(query): Query<LoginQuery>,
) -> Response {
    let (Some(client_id), Some(_)) = (&state.google_client_id, &state.google_client_secret) else {
        return (StatusCode::SERVICE_UNAVAILABLE, Html(
            "<h2>Google OAuth not configured</h2><p>Set <code>google_client_id</code> and <code>google_client_secret</code> in settings.toml</p>"
        )).into_response();
    };

    let redirect_uri = format!("http://127.0.0.1:{}/auth/callback", state.daemon_port);

    // Encode discord_id and redirect into state param
    let oauth_state = serde_json::json!({
        "discord_id": query.discord_id,
        "redirect": query.redirect,
    });
    let state_encoded = urlencoding::encode(&oauth_state.to_string());

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=email%20profile&access_type=offline&prompt=consent&state={}",
        urlencoding::encode(client_id),
        urlencoding::encode(&redirect_uri),
        state_encoded,
    );

    Redirect::temporary(&auth_url).into_response()
}

/// GET /auth/callback — handle Google OAuth callback, create user, issue token
pub async fn auth_callback(
    State(state): State<AuthState>,
    Query(query): Query<CallbackQuery>,
) -> Response {
    if let Some(error) = &query.error {
        return Html(format!(
            "<h2>Authentication Failed</h2><p>{error}</p>"
        )).into_response();
    }

    let Some(code) = &query.code else {
        return (StatusCode::BAD_REQUEST, "missing code parameter").into_response();
    };

    let (Some(client_id), Some(client_secret)) = (&state.google_client_id, &state.google_client_secret) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "OAuth not configured").into_response();
    };

    let redirect_uri = format!("http://127.0.0.1:{}/auth/callback", state.daemon_port);

    // Exchange code for tokens
    let client = reqwest::Client::new();
    let token_resp = client.post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await;

    let token_resp = match token_resp {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("token exchange failed: {e}")).into_response(),
    };

    let token_json: serde_json::Value = match token_resp.json().await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("invalid token response: {e}")).into_response(),
    };

    let Some(access_token) = token_json["access_token"].as_str() else {
        return (StatusCode::BAD_GATEWAY, "no access_token in response").into_response();
    };

    // Fetch user profile from Google
    let profile_resp = client.get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await;

    let profile: serde_json::Value = match profile_resp {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("invalid profile: {e}")).into_response(),
        },
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("profile fetch failed: {e}")).into_response(),
    };

    let Some(email) = profile["email"].as_str() else {
        return (StatusCode::BAD_GATEWAY, "no email in Google profile").into_response();
    };

    // Create or get user
    let user = match state.user_store.get_or_create_user_by_email(email).await {
        Ok(u) => u,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("user creation failed: {e}")).into_response(),
    };

    // Parse state to get discord_id if present
    if let Some(state_str) = &query.state {
        if let Ok(decoded) = urlencoding::decode(state_str) {
            if let Ok(state_json) = serde_json::from_str::<serde_json::Value>(&decoded) {
                if let Some(discord_id) = state_json["discord_id"].as_str() {
                    let _ = state.user_store.map_discord_user(discord_id, &user.id).await;
                    tracing::info!(discord_id, email, "Discord user mapped to UCM user");
                }
            }
        }
    }

    // Determine tier
    let is_admin = state.admin_email.as_deref() == Some(email);

    // Issue a user token
    let token = match state.user_store.create_user_token(&user.id).await {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("token creation failed: {e}")).into_response(),
    };

    tracing::info!(email, is_admin, "user authenticated via Google OAuth");

    // Return a success page with the token
    Html(format!(r#"<!DOCTYPE html>
<html>
<head><title>Authentication Successful</title>
<style>
  body {{ font-family: -apple-system, system-ui, sans-serif; max-width: 600px; margin: 80px auto; padding: 0 20px; background: #1a1a2e; color: #e0e0e0; text-align: center; }}
  h1 {{ color: #14b8a6; }}
  .token {{ background: #2a2a3e; padding: 12px; border-radius: 6px; font-family: monospace; word-break: break-all; margin: 20px 0; font-size: 0.9em; }}
  .info {{ color: #888; font-size: 0.85em; }}
</style>
</head>
<body>
  <h1>Authenticated!</h1>
  <p>Welcome, <strong>{email}</strong>{}</p>
  <p class="info">You can close this tab and return to the app.</p>
</body>
</html>"#,
        if is_admin { " (admin)" } else { "" },
    )).into_response()
}
