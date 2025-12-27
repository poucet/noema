//! Application settings management

use crate::{crypto, PathManager};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

/// Application settings stored in settings.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    /// User email for database user identification
    pub user_email: Option<String>,
    /// Default model ID (e.g., "claude/models/claude-sonnet-4-5-20250929")
    pub default_model: Option<String>,
    /// Encrypted API keys (provider name -> encrypted key)
    #[serde(default)]
    pub api_keys: HashMap<String, String>,
    /// Favorite model IDs for quick access (e.g., ["claude/claude-sonnet-4-5", "openai/gpt-4o"])
    #[serde(default)]
    pub favorite_models: Vec<String>,
}

impl Settings {
    /// Load settings from the settings file, or return defaults if not found
    pub fn load() -> Self {
        let Some(path) = PathManager::settings_path() else {
            return Self::default();
        };

        let Ok(content) = fs::read_to_string(&path) else {
            return Self::default();
        };

        toml::from_str(&content).unwrap_or_default()
    }

    /// Save settings to the settings file
    pub fn save(&self) -> Result<(), String> {
        let path = PathManager::settings_path().ok_or("Could not determine settings path")?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        fs::write(&path, content).map_err(|e| format!("Failed to write settings: {}", e))?;
        Ok(())
    }

    /// Get a decrypted API key for a provider.
    /// Returns None if not set or decryption fails.
    pub fn get_api_key(&self, provider: &str) -> Option<String> {
        self.api_keys
            .get(provider)
            .and_then(|encrypted| crypto::decrypt_string(encrypted).ok())
    }

    /// Set an API key for a provider (encrypts before storing).
    pub fn set_api_key(&mut self, provider: &str, api_key: &str) -> Result<(), String> {
        let encrypted = crypto::encrypt_string(api_key)?;
        self.api_keys.insert(provider.to_string(), encrypted);
        Ok(())
    }

    /// Remove an API key for a provider.
    pub fn remove_api_key(&mut self, provider: &str) {
        self.api_keys.remove(provider);
    }

    /// Check if an API key is set for a provider.
    pub fn has_api_key(&self, provider: &str) -> bool {
        self.api_keys.contains_key(provider)
    }

    /// Get the list of providers with configured API keys.
    pub fn configured_providers(&self) -> Vec<String> {
        self.api_keys.keys().cloned().collect()
    }

    /// Get favorite model IDs.
    pub fn get_favorite_models(&self) -> &[String] {
        &self.favorite_models
    }

    /// Toggle a model as favorite. Returns true if now favorited, false if removed.
    pub fn toggle_favorite_model(&mut self, model_id: &str) -> bool {
        if let Some(pos) = self.favorite_models.iter().position(|m| m == model_id) {
            self.favorite_models.remove(pos);
            false
        } else {
            self.favorite_models.push(model_id.to_string());
            true
        }
    }

    /// Check if a model is favorited.
    pub fn is_favorite_model(&self, model_id: &str) -> bool {
        self.favorite_models.iter().any(|m| m == model_id)
    }
}
