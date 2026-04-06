//! Admin API endpoints — settings, user management, setup status.
//!
//! These routes are served under `/admin/api/*` and are accessible without
//! Bearer auth (same as the admin page — localhost only, Google OAuth in Stage 5).

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};

use simply_core::storage::ids::UserId;
use simply_core::storage::traits::UserStore;

/// Shared state for admin API routes.
#[derive(Clone)]
pub struct AdminState {
    pub user_store: Arc<dyn UserStore>,
    pub daemon_secret: Arc<str>,
}

// ---------------------------------------------------------------------------
// Setup status
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct SetupStatus {
    pub is_configured: bool,
    pub has_admin_email: bool,
    pub has_google_oauth: bool,
    pub has_api_keys: Vec<String>,
    pub daemon_port: u16,
}

pub async fn get_setup_status(State(_state): State<AdminState>) -> Json<SetupStatus> {
    let settings = config::Settings::load();
    Json(SetupStatus {
        is_configured: settings.admin_email.is_some(),
        has_admin_email: settings.admin_email.is_some(),
        has_google_oauth: settings.google_client_id.is_some() && settings.google_client_secret.is_some(),
        has_api_keys: settings.configured_providers(),
        daemon_port: settings.daemon_port.unwrap_or(config::DEFAULT_DAEMON_PORT),
    })
}

// ---------------------------------------------------------------------------
// Settings management
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct SettingsResponse {
    pub admin_email: Option<String>,
    pub user_email: Option<String>,
    pub default_model: Option<String>,
    pub daemon_port: Option<u16>,
    pub has_google_oauth: bool,
    pub api_keys: Vec<String>,
}

pub async fn get_settings(State(_state): State<AdminState>) -> Json<SettingsResponse> {
    let settings = config::Settings::load();
    Json(SettingsResponse {
        admin_email: settings.admin_email,
        user_email: settings.user_email,
        default_model: settings.default_model,
        daemon_port: settings.daemon_port,
        has_google_oauth: settings.google_client_id.is_some(),
        api_keys: settings.configured_providers(),
    })
}

#[derive(serde::Deserialize)]
pub struct UpdateSettingsRequest {
    pub admin_email: Option<String>,
    pub user_email: Option<String>,
    pub default_model: Option<String>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
}

pub async fn update_settings(
    State(_state): State<AdminState>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Response {
    let mut settings = config::Settings::load();

    if let Some(v) = req.admin_email { settings.admin_email = Some(v); }
    if let Some(v) = req.user_email { settings.user_email = Some(v); }
    if let Some(v) = req.default_model { settings.default_model = Some(v); }
    if let Some(v) = req.google_client_id { settings.google_client_id = Some(v); }
    if let Some(v) = req.google_client_secret { settings.google_client_secret = Some(v); }

    match settings.save() {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

// ---------------------------------------------------------------------------
// API key management
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct SetApiKeyRequest {
    pub provider: String,
    pub api_key: String,
}

pub async fn set_api_key(
    State(_state): State<AdminState>,
    Json(req): Json<SetApiKeyRequest>,
) -> Response {
    let mut settings = config::Settings::load();
    if let Err(e) = settings.set_api_key(&req.provider, &req.api_key) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
    }
    match settings.save() {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct RemoveApiKeyRequest {
    pub provider: String,
}

pub async fn remove_api_key(
    State(_state): State<AdminState>,
    Json(req): Json<RemoveApiKeyRequest>,
) -> Response {
    let mut settings = config::Settings::load();
    settings.remove_api_key(&req.provider);
    match settings.save() {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

// ---------------------------------------------------------------------------
// User management
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

pub async fn list_users(State(state): State<AdminState>) -> Response {
    match state.user_store.list_users().await {
        Ok(users) => {
            let infos: Vec<UserInfo> = users.iter().map(|u| UserInfo {
                id: u.id.as_str().to_string(),
                email: u.email.clone(),
            }).collect();
            Json(infos).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct CreateTokenRequest {
    pub user_id: String,
}

pub async fn create_user_token(
    State(state): State<AdminState>,
    Json(req): Json<CreateTokenRequest>,
) -> Response {
    let uid = UserId::from_string(req.user_id);
    match state.user_store.create_user_token(&uid).await {
        Ok(token) => Json(serde_json::json!({ "token": token })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct RevokeTokensRequest {
    pub user_id: String,
}

pub async fn revoke_user_tokens(
    State(state): State<AdminState>,
    Json(req): Json<RevokeTokensRequest>,
) -> Response {
    let uid = UserId::from_string(req.user_id);
    match state.user_store.revoke_user_tokens(&uid).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
}

pub async fn create_user(
    State(state): State<AdminState>,
    Json(req): Json<CreateUserRequest>,
) -> Response {
    match state.user_store.get_or_create_user_by_email(&req.email).await {
        Ok(user) => Json(serde_json::json!({
            "id": user.id.as_str(),
            "email": user.email,
        })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
