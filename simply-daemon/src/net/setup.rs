//! First-run setup wizard.
//!
//! A single self-contained page (see `setup.html`) served only while the daemon
//! is **unconfigured** and only with the correct one-time token. It writes the
//! per-server config — `settings.toml`, `lumina.toml`, `oauth_providers.toml` —
//! then exits so the supervisor (Docker `restart: unless-stopped`) brings the
//! process back up already configured. Once configured, `/setup` 404s forever.
//!
//! Security: when reached through the reverse proxy the connection is loopback,
//! which the auth middleware treats as admin — so the token check in these
//! handlers is the actual gate, independent of `RequestUser`.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Json, Response};
use serde::Deserialize;

use llm::{GeneralModelProvider, ModelProvider};

use crate::oauth::providers::{self, OAuthProvider};

const SETUP_HTML: &str = include_str!("setup.html");

/// Shared state for the setup routes. `token` is `Some` only while the daemon
/// is unconfigured at boot; `None` means no setup page is ever served.
#[derive(Clone)]
pub struct SetupState {
    pub token: Option<Arc<str>>,
}

/// True once an owner email is recorded — the existing "is configured" signal
/// (mirrors `admin_api::get_setup_status` / `auth_routes::auth_status`).
fn is_configured() -> bool {
    config::Settings::load()
        .user_email
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty())
}

/// Constant-time token comparison against the live one-time token.
fn token_ok(state: &SetupState, provided: Option<&str>) -> bool {
    let (Some(tok), Some(p)) = (state.token.as_deref(), provided) else {
        return false;
    };
    let (a, b) = (tok.as_bytes(), p.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Deserialize)]
pub struct PageQuery {
    token: Option<String>,
}

/// GET /setup — serve the wizard, or 404 if configured / wrong token.
pub async fn page(State(state): State<SetupState>, Query(q): Query<PageQuery>) -> Response {
    if is_configured() || !token_ok(&state, q.token.as_deref()) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let public_url = config::Settings::load().public_url.unwrap_or_default();
    let html = SETUP_HTML
        .replace("{{PUBLIC_URL}}", &public_url)
        .replace("{{INVITE_PERMISSIONS}}", &config::DISCORD_INVITE_PERMISSIONS.to_string())
        .replace("{{INVITE_SCOPES}}", config::DISCORD_INVITE_SCOPES);
    Html(html).into_response()
}

#[derive(Deserialize)]
pub struct ApiKeyEntry {
    provider: String,
    api_key: String,
}

#[derive(Deserialize)]
pub struct CompleteReq {
    email: String,
    default_model: String,
    /// One or more LLM provider keys (mistral, claude, openai, …).
    #[serde(default)]
    api_keys: Vec<ApiKeyEntry>,
    discord_bot_token: String,
    #[serde(default)]
    owner_id: Option<u64>,
    #[serde(default)]
    guild_ids: Option<Vec<u64>>,
    #[serde(default)]
    model_id: Option<String>,
    google_client_id: String,
    google_client_secret: String,
    #[serde(default)]
    public_url: Option<String>,
}

/// POST /setup/api/complete — write all config, then restart into configured mode.
pub async fn complete(
    State(state): State<SetupState>,
    headers: HeaderMap,
    Json(req): Json<CompleteReq>,
) -> Response {
    let provided = headers.get("X-Setup-Token").and_then(|v| v.to_str().ok());
    if is_configured() || !token_ok(&state, provided) {
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Err(e) = apply(req) {
        tracing::error!(error = %e, "setup: failed to write config");
        return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
    }

    // Respond first, then exit so the supervisor restarts us already configured;
    // on the next boot the Discord token in lumina.toml lets the bot connect.
    tokio::spawn(async {
        // Wait long enough for the 200 to flow back through nginx + Cloudflare to
        // the browser before the process exits (an early exit severs the
        // connection → the user sees a 502 instead of the success screen).
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        tracing::info!("setup complete — restarting to apply configuration");
        std::process::exit(0);
    });

    Json(serde_json::json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
pub struct ModelsReq {
    provider: String,
    api_key: String,
}

/// POST /setup/api/models — list a provider's real model ids using the
/// just-entered key. Done server-side because browsers can't call LLM APIs
/// directly (CORS), and it gives canonical ids so users don't hand-type them.
pub async fn models(
    State(state): State<SetupState>,
    headers: HeaderMap,
    Json(req): Json<ModelsReq>,
) -> Response {
    let provided = headers.get("X-Setup-Token").and_then(|v| v.to_str().ok());
    if is_configured() || !token_ok(&state, provided) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (provider, key) = (req.provider.trim(), req.api_key.trim());
    if provider.is_empty() || key.is_empty() {
        return (StatusCode::BAD_REQUEST, "provider and api_key required").into_response();
    }
    match list_provider_models(provider, key).await {
        Ok(ids) => Json(serde_json::json!({ "models": ids })).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, format!("could not list models: {e}")).into_response(),
    }
}

/// Build a provider from the raw key (no settings needed) and list its models
/// as full `provider/model` ids.
async fn list_provider_models(provider: &str, api_key: &str) -> anyhow::Result<Vec<String>> {
    let p = GeneralModelProvider::from_name_with_key(provider, Some(api_key))?;
    let mut models = p.list_models().await?;
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models.into_iter().map(|d| format!("{provider}/{}", d.id)).collect())
}

/// Persist the wizard's answers across settings.toml / lumina.toml / oauth_providers.toml.
fn apply(req: CompleteReq) -> Result<(), String> {
    let trim_opt = |s: Option<String>| {
        s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
    };

    // settings.toml — preserves public_url / daemon_secret written at boot.
    let mut settings = config::Settings::load();
    settings.user_email = Some(req.email.trim().to_string());
    settings.default_model = Some(req.default_model.trim().to_string());
    if let Some(pu) = trim_opt(req.public_url) {
        settings.public_url = Some(pu);
    }
    for entry in &req.api_keys {
        let (provider, key) = (entry.provider.trim(), entry.api_key.trim());
        if !provider.is_empty() && !key.is_empty() {
            settings.set_api_key(provider, key)?;
        }
    }
    settings.save()?;

    // lumina.toml — Discord identity.
    let mut lumina = config::LuminaConfig::load();
    lumina.discord.bot_token = req.discord_bot_token.trim().to_string();
    lumina.discord.owner_id = req.owner_id;
    if let Some(ids) = req.guild_ids {
        lumina.discord.guild_ids = ids;
    }
    lumina.discord.model_id = trim_opt(req.model_id);
    lumina.save()?;

    // oauth_providers.toml — Google client credentials.
    let mut all = providers::load_providers();
    let google = all.entry("google".to_string()).or_insert_with(default_google_provider);
    google.client_id = req.google_client_id.trim().to_string();
    google.client_secret = trim_opt(Some(req.google_client_secret));
    providers::save_providers(&all).map_err(|e| e.to_string())?;

    Ok(())
}

/// Default Google provider entry (matches `oauth/defaults/oauth_providers.toml`),
/// used only if `oauth_providers.toml` has no `[google]` yet.
fn default_google_provider() -> OAuthProvider {
    OAuthProvider {
        display_name: "Google".to_string(),
        authorization_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
        token_url: "https://oauth2.googleapis.com/token".to_string(),
        userinfo_url: Some("https://www.googleapis.com/oauth2/v2/userinfo".to_string()),
        client_id: String::new(),
        client_secret: None,
    }
}
