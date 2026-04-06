//! Auth routes — Google OAuth login and user token issuance.
//!
//! Routes:
//! - GET  /auth/login?discord_id=X  — start Google OAuth flow
//! - GET  /auth/callback            — OAuth callback, creates user, issues session

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::http::StatusCode;

use simply_core::storage::traits::UserStore;

/// Built-in Google OAuth credentials for localhost development.
/// These are safe to ship — Google's security model allows public client IDs
/// for localhost redirect URIs.
const BUILTIN_GOOGLE_CLIENT_ID: Option<&str> = None; // TODO: register and embed
const BUILTIN_GOOGLE_CLIENT_SECRET: Option<&str> = None;

/// Shared state for auth routes.
#[derive(Clone)]
pub struct AuthState {
    pub user_store: Arc<dyn UserStore>,
    pub daemon_port: u16,
}

impl AuthState {
    /// Resolve Google OAuth credentials: env vars > settings > built-in.
    fn google_creds(&self) -> Option<(String, String)> {
        // Priority 1: env vars (cloud deployments)
        if let (Ok(id), Ok(secret)) = (
            std::env::var("GOOGLE_CLIENT_ID"),
            std::env::var("GOOGLE_CLIENT_SECRET"),
        ) {
            return Some((id, secret));
        }

        // Priority 2: settings.toml
        let settings = config::Settings::load();
        if let (Some(id), Some(secret)) = (settings.google_client_id, settings.google_client_secret) {
            return Some((id, secret));
        }

        // Priority 3: built-in (localhost)
        if let (Some(id), Some(secret)) = (BUILTIN_GOOGLE_CLIENT_ID, BUILTIN_GOOGLE_CLIENT_SECRET) {
            return Some((id.to_string(), secret.to_string()));
        }

        None
    }

    fn redirect_uri(&self) -> String {
        std::env::var("OAUTH_REDIRECT_URI")
            .unwrap_or_else(|_| format!("http://127.0.0.1:{}/auth/callback", self.daemon_port))
    }

    fn has_google_oauth(&self) -> bool {
        self.google_creds().is_some()
    }
}

#[derive(serde::Deserialize)]
pub struct LoginQuery {
    pub discord_id: Option<String>,
    pub redirect: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// GET /auth/status — check if Google OAuth is available (used by admin page)
pub async fn auth_status(State(state): State<AuthState>) -> Json<serde_json::Value> {
    let settings = config::Settings::load();
    Json(serde_json::json!({
        "google_oauth_available": state.has_google_oauth(),
        "admin_email": settings.admin_email,
        "is_setup_complete": settings.admin_email.is_some(),
    }))
}

use axum::response::Json;

/// GET /auth/login — redirect to Google OAuth
pub async fn auth_login(
    State(state): State<AuthState>,
    Query(query): Query<LoginQuery>,
) -> Response {
    let Some((client_id, _)) = state.google_creds() else {
        return (StatusCode::SERVICE_UNAVAILABLE, Html(
            "<h2>Google OAuth not configured</h2>\
             <p>Set <code>GOOGLE_CLIENT_ID</code> and <code>GOOGLE_CLIENT_SECRET</code> environment variables, \
             or configure them in the admin settings.</p>"
        )).into_response();
    };

    let redirect_uri = state.redirect_uri();

    // Encode discord_id and redirect into state param
    let oauth_state = serde_json::json!({
        "discord_id": query.discord_id,
        "redirect": query.redirect,
    });
    let state_string = oauth_state.to_string();
    let state_encoded = urlencoding::encode(&state_string);

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
         client_id={}&redirect_uri={}&response_type=code&\
         scope=email%20profile&access_type=offline&prompt=consent&state={}",
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect_uri),
        state_encoded,
    );

    Redirect::temporary(&auth_url).into_response()
}

/// GET /auth/callback — handle Google OAuth callback
///
/// First sign-in becomes admin. Creates UCM user. Issues user token.
/// Redirects to admin page on success.
pub async fn auth_callback(
    State(state): State<AuthState>,
    Query(query): Query<CallbackQuery>,
) -> Response {
    if let Some(error) = &query.error {
        return Html(format!(
            r#"<!DOCTYPE html><html><head><title>Auth Failed</title>
            <style>body {{ font-family: system-ui; max-width: 600px; margin: 80px auto; text-align: center; background: #1a1a2e; color: #e0e0e0; }}</style>
            </head><body><h2 style="color:#ef4444">Authentication Failed</h2><p>{error}</p>
            <a href="/" style="color:#14b8a6">Back to dashboard</a></body></html>"#
        )).into_response();
    }

    let Some(code) = &query.code else {
        return (StatusCode::BAD_REQUEST, "missing code parameter").into_response();
    };

    let Some((client_id, client_secret)) = state.google_creds() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "OAuth not configured").into_response();
    };

    let redirect_uri = state.redirect_uri();

    // Exchange code for tokens
    let client = reqwest::Client::new();
    let token_resp = match client.post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("token exchange failed: {e}")).into_response(),
    };

    let token_json: serde_json::Value = match token_resp.json().await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("invalid token response: {e}")).into_response(),
    };

    let Some(access_token) = token_json["access_token"].as_str() else {
        let error = token_json.get("error_description")
            .or(token_json.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("no access_token in response");
        return (StatusCode::BAD_GATEWAY, error.to_string()).into_response();
    };

    // Fetch user profile
    let profile: serde_json::Value = match client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
    {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("invalid profile: {e}")).into_response(),
        },
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("profile fetch failed: {e}")).into_response(),
    };

    let Some(email) = profile["email"].as_str() else {
        return (StatusCode::BAD_GATEWAY, "no email in Google profile").into_response();
    };

    // Create or get UCM user
    let user = match state.user_store.get_or_create_user_by_email(email).await {
        Ok(u) => u,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("user creation failed: {e}")).into_response(),
    };

    // First sign-in becomes admin
    let mut settings = config::Settings::load();
    let is_first_user = settings.admin_email.is_none();
    if is_first_user {
        settings.admin_email = Some(email.to_string());
        settings.user_email = Some(email.to_string());
        if let Err(e) = settings.save() {
            tracing::error!("failed to save admin_email: {e}");
        }
        tracing::info!(email, "first sign-in — set as admin");
    }

    let is_admin = settings.admin_email.as_deref() == Some(email);

    // Map Discord user if discord_id was passed in state
    if let Some(state_str) = &query.state {
        if let Ok(decoded) = urlencoding::decode(state_str) {
            if let Ok(state_json) = serde_json::from_str::<serde_json::Value>(&decoded) {
                if let Some(discord_id) = state_json["discord_id"].as_str() {
                    if !discord_id.is_empty() {
                        let _ = state.user_store.map_discord_user(discord_id, &user.id).await;
                        tracing::info!(discord_id, email, "Discord user mapped");
                    }
                }
            }
        }
    }

    // Issue user token
    let token = match state.user_store.create_user_token(&user.id).await {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("token creation failed: {e}")).into_response(),
    };

    tracing::info!(email, is_admin, user_id = %user.id, "Google OAuth complete");

    // Set session cookie and redirect to admin page
    let cookie = format!("simply_token={}; Path=/; HttpOnly; SameSite=Lax", token);
    let mut response = Redirect::temporary("/").into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        cookie.parse().unwrap(),
    );
    response
}
