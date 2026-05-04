//! Application settings management

use crate::{crypto, PathManager};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

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
    /// Port for the OAuth callback server (default: 9876)
    pub oauth_callback_port: Option<u16>,
    /// Port for the daemon server (default: 9800)
    pub daemon_port: Option<u16>,
    /// Shared secret for client authentication (auto-generated on first run)
    pub daemon_secret: Option<String>,
    /// Embedding provider configuration
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    /// Public URL for OAuth callbacks (defaults to http://localhost:{daemon_port}).
    /// Set this when hosting the daemon on a remote server.
    pub public_url: Option<String>,
    /// Root directory for user-authored Markdown vault files.
    pub vault_root: Option<PathBuf>,
}

/// Embedding provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Provider name (e.g. "mistral", "openai", "gemini", "claude", "ollama", "local")
    pub provider: String,
    /// Model name (e.g. "mistral-embed", "text-embedding-3-small")
    pub model: String,
    /// Maximum chunk size in characters (~4 chars per token)
    pub chunk_size: usize,
    /// Overlap between consecutive chunks in characters
    pub chunk_overlap: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "local".to_string(),
            model: "bge-small-en-v1.5".to_string(),
            chunk_size: 16000,   // ~4096 tokens
            chunk_overlap: 512,  // ~128 tokens
        }
    }
}

/// Default port for the daemon server.
pub const DEFAULT_DAEMON_PORT: u16 = 9800;

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
    /// Get an API key for a provider (plaintext).
    /// Falls back to trying decryption for backward compatibility with old encrypted keys.
    pub fn get_api_key(&self, provider: &str) -> Option<String> {
        let value = self.api_keys.get(provider)?;
        // Try decryption first (backward compat), fall back to plaintext
        crypto::decrypt_string(value).ok().or_else(|| Some(value.clone()))
    }

    /// Set an API key for a provider (stored as plaintext).
    pub fn set_api_key(&mut self, provider: &str, api_key: &str) -> Result<(), String> {
        self.api_keys.insert(provider.to_string(), api_key.to_string());
        Ok(())
    }

    /// Remove an API key for a provider.
    pub fn remove_api_key(&mut self, provider: &str) {
        self.api_keys.remove(provider);
    }

    /// Resolve the Markdown vault root, falling back to the default data-dir
    /// location when no explicit path is configured.
    pub fn vault_root(&self) -> Option<PathBuf> {
        self.vault_root
            .clone()
            .or_else(PathManager::default_vault_root)
    }

    /// Ensure the resolved Markdown vault root exists.
    pub fn ensure_vault_root_exists(&self) -> std::io::Result<Option<PathBuf>> {
        let Some(path) = self.vault_root() else {
            return Ok(None);
        };
        fs::create_dir_all(&path)?;
        Ok(Some(path))
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

    /// Returns the daemon secret, generating and persisting one if it doesn't exist.
    pub fn ensure_daemon_secret(&mut self) -> &str {
        if self.daemon_secret.is_none() {
            let mut bytes = [0u8; 32];
            rand::rng().fill(&mut bytes);
            let secret = URL_SAFE_NO_PAD.encode(bytes);
            self.daemon_secret = Some(secret);
            if let Err(e) = self.save() {
                eprintln!("failed to save daemon_secret: {e}");
            }
        }
        self.daemon_secret.as_deref().unwrap()
    }
}
