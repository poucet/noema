//! Lumina bot configuration

use crate::PathManager;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LuminaConfig {
    #[serde(default)]
    pub discord: DiscordConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordConfig {
    /// Discord bot token — can also be set via DISCORD_BOT_TOKEN env var
    #[serde(default)]
    pub bot_token: String,
}

impl LuminaConfig {
    /// Load from lumina.toml, falling back to defaults.
    /// The DISCORD_BOT_TOKEN env var takes precedence over the file value.
    pub fn load() -> Self {
        let mut cfg: Self = PathManager::lumina_config_path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();

        if let Ok(token) = std::env::var("DISCORD_BOT_TOKEN") {
            if !token.is_empty() {
                cfg.discord.bot_token = token;
            }
        }

        cfg
    }

    pub fn bot_token(&self) -> Option<&str> {
        if self.discord.bot_token.is_empty() {
            None
        } else {
            Some(&self.discord.bot_token)
        }
    }
}
