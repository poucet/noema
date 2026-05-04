//! Admin API endpoints — settings, user management, setup status.
//!
//! Served under `/admin/api/*`. Accessible from localhost without Bearer auth.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};

use simply_core::storage::traits::UserStore;

/// Shared state for admin API routes.
#[derive(Clone)]
pub struct AdminState {
    pub user_store: Arc<dyn UserStore>,
}

// ---------------------------------------------------------------------------
// Setup status
// ---------------------------------------------------------------------------

pub async fn get_setup_status(State(_state): State<AdminState>) -> Json<serde_json::Value> {
    let settings = config::Settings::load();
    Json(serde_json::json!({
        "is_configured": settings.user_email.is_some(),
        "api_keys": settings.configured_providers(),
        "daemon_port": settings.daemon_port.unwrap_or(config::DEFAULT_DAEMON_PORT),
    }))
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

pub async fn get_settings(State(_state): State<AdminState>) -> Json<serde_json::Value> {
    let settings = config::Settings::load();
    Json(serde_json::json!({
        "user_email": settings.user_email,
        "default_model": settings.default_model,
        "daemon_port": settings.daemon_port,
        "vault_root": settings.vault_root().map(|p| p.to_string_lossy().to_string()),
        "api_keys": settings.configured_providers(),
    }))
}

#[derive(serde::Deserialize)]
pub struct UpdateSettingsRequest {
    pub user_email: Option<String>,
    pub default_model: Option<String>,
    pub vault_root: Option<String>,
}

pub async fn update_settings(
    State(_state): State<AdminState>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Response {
    let mut settings = config::Settings::load();
    if let Some(v) = req.user_email {
        settings.user_email = Some(v);
    }
    if let Some(v) = req.default_model {
        settings.default_model = Some(v);
    }
    if let Some(v) = req.vault_root {
        let trimmed = v.trim();
        settings.vault_root = if trimmed.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(trimmed))
        };
        if let Err(e) = settings.ensure_vault_root_exists() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create vault root: {e}"),
            )
                .into_response();
        }
    }
    match settings.save() {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

// ---------------------------------------------------------------------------
// API keys
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct ApiKeyRequest {
    pub provider: String,
    pub api_key: Option<String>,
}

pub async fn set_api_key(State(_state): State<AdminState>, Json(req): Json<ApiKeyRequest>) -> Response {
    let mut settings = config::Settings::load();
    if let Err(e) = settings.set_api_key(&req.provider, req.api_key.as_deref().unwrap_or("")) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    match settings.save() {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub async fn remove_api_key(
    State(_state): State<AdminState>,
    axum::extract::Path(provider): axum::extract::Path<String>,
) -> Response {
    let mut settings = config::Settings::load();
    settings.remove_api_key(&provider);
    match settings.save() {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

pub async fn list_users(State(state): State<AdminState>) -> Response {
    match state.user_store.list_users().await {
        Ok(users) => {
            let infos: Vec<serde_json::Value> = users.iter().map(|u| serde_json::json!({
                "id": u.id.as_str(),
                "email": u.email,
            })).collect();
            Json(infos).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
}

pub async fn delete_user(
    State(state): State<AdminState>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
) -> Response {
    let id = simply_core::storage::ids::UserId::from_string(&user_id);
    match state.user_store.delete_user(&id).await {
        Ok(true) => Json(serde_json::json!({ "ok": true })).into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "user not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn create_user(State(state): State<AdminState>, Json(req): Json<CreateUserRequest>) -> Response {
    match state.user_store.get_or_create_user_by_email(&req.email).await {
        Ok(user) => Json(serde_json::json!({
            "id": user.id.as_str(),
            "email": user.email,
        })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
