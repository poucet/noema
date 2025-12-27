//! Settings commands

use config::Settings;
use llm::registry::list_providers;
use std::collections::HashMap;

/// Get the current user email setting
#[tauri::command]
pub fn get_user_email() -> Option<String> {
    Settings::load().user_email
}

/// Set the user email setting
#[tauri::command]
pub fn set_user_email(email: String) -> Result<(), String> {
    let mut settings = Settings::load();
    settings.user_email = Some(email);
    settings.save()
}

/// Get API key status for all providers (provider name -> has key configured)
#[tauri::command]
pub fn get_api_key_status() -> HashMap<String, bool> {
    let settings = Settings::load();
    list_providers()
        .iter()
        .map(|info| (info.name.to_string(), settings.has_api_key(info.name)))
        .collect()
}

/// Set an API key for a provider (encrypts and saves)
#[tauri::command]
pub fn set_api_key(provider: String, api_key: String) -> Result<(), String> {
    let mut settings = Settings::load();
    settings.set_api_key(&provider, &api_key)?;
    settings.save()
}

/// Remove an API key for a provider
#[tauri::command]
pub fn remove_api_key(provider: String) -> Result<(), String> {
    let mut settings = Settings::load();
    settings.remove_api_key(&provider);
    settings.save()
}

/// Get provider info (name, whether it requires API key, env var name)
#[tauri::command]
pub fn get_provider_info() -> Vec<ProviderInfoResponse> {
    list_providers()
        .iter()
        .map(|info| ProviderInfoResponse {
            name: info.name.to_string(),
            requires_api_key: info.api_key_env.is_some(),
            api_key_env: info.api_key_env.map(|s| s.to_string()),
        })
        .collect()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfoResponse {
    pub name: String,
    pub requires_api_key: bool,
    pub api_key_env: Option<String>,
}
