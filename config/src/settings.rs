//! Application settings management

use crate::PathManager;
use serde::{Deserialize, Serialize};
use std::fs;

/// Application settings stored in settings.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    /// User email for database user identification
    pub user_email: Option<String>,
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
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
        }

        let content = toml::to_string_pretty(self).map_err(|e| format!("Failed to serialize settings: {}", e))?;
        fs::write(&path, content).map_err(|e| format!("Failed to write settings: {}", e))?;
        Ok(())
    }
}
