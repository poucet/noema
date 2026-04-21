//! Lumina Discord skill — exposes Discord actions as skill tools.
//!
//! Tools are defined with rmcp's `#[tool]` macros (for metadata + param types),
//! and dispatched via `#[skill_router]` which generates the `Skill` trait impl.
//! No HTTP MCP server — the daemon calls into this skill over WS reverse-RPC
//! or in-process (when Lumina embeds the daemon).

pub mod param;
mod tools;

use serenity::all::Cache;
use serenity::http::Http;
use std::sync::{Arc, OnceLock};

use crate::voice::VoiceManager;

// ---------------------------------------------------------------------------
// Skill
// ---------------------------------------------------------------------------

/// Discord skill — provides tools for interacting with Discord
/// channels, messages, guilds, and users.
#[derive(Clone)]
pub struct DiscordSkill {
    inner: Arc<DiscordSkillInner>,
}

struct DiscordSkillInner {
    http: Arc<Http>,
    cache: OnceLock<Arc<Cache>>,
    voice_manager: OnceLock<Arc<VoiceManager>>,
    /// Instructions with guild/channel map, refreshed when channels change.
    #[allow(dead_code)]
    instructions: std::sync::RwLock<String>,
}

impl DiscordSkill {
    const BASE_INSTRUCTIONS: &str =
        "Lumina Discord skill. Provides tools for interacting with Discord \
         channels, messages, guilds, and users.";

    pub fn new(http: Arc<Http>) -> Self {
        let inner = Arc::new(DiscordSkillInner {
            http,
            cache: OnceLock::new(),
            voice_manager: OnceLock::new(),
            instructions: std::sync::RwLock::new(Self::BASE_INSTRUCTIONS.to_string()),
        });
        Self { inner }
    }

    /// Inject the serenity cache once the gateway is connected.
    /// Called from the `ready` event handler.
    pub fn set_cache(&self, cache: Arc<Cache>) {
        let _ = self.inner.cache.set(cache);
        self.refresh_instructions();
    }

    /// Inject the voice manager so voice-related tools (e.g. leave_voice)
    /// can act on active sessions.
    pub fn set_voice_manager(&self, voice_manager: Arc<VoiceManager>) {
        let _ = self.inner.voice_manager.set(voice_manager);
    }

    pub(crate) fn voice_manager(&self) -> Option<&Arc<VoiceManager>> {
        self.inner.voice_manager.get()
    }

    /// Rebuild the instructions string from the current gateway cache.
    /// Call this when guild/channel state may have changed.
    pub fn refresh_instructions(&self) {
        let Some(cache) = self.inner.cache.get() else { return };

        let mut text = String::from(Self::BASE_INSTRUCTIONS);
        text.push_str("\n\n## Available Discord Servers & Channels\n");

        for guild_id in cache.guilds() {
            let Some(guild) = cache.guild(guild_id) else { continue };
            text.push_str(&format!("\n### {} (guild_id: {})\n", guild.name, guild_id.get()));

            let mut categories: std::collections::BTreeMap<Option<serenity::model::id::ChannelId>, Vec<&serenity::model::channel::GuildChannel>> =
                std::collections::BTreeMap::new();

            for ch in guild.channels.values() {
                if ch.kind == serenity::model::channel::ChannelType::Category {
                    continue;
                }
                categories.entry(ch.parent_id).or_default().push(ch);
            }

            for (parent_id, channels) in &categories {
                if let Some(pid) = parent_id {
                    if let Some(cat) = guild.channels.get(pid) {
                        text.push_str(&format!("\n**{}**\n", cat.name));
                    }
                }
                let mut sorted: Vec<_> = channels.iter().collect();
                sorted.sort_by_key(|ch| ch.position);
                for ch in sorted {
                    let kind = match ch.kind {
                        serenity::model::channel::ChannelType::Voice => "voice",
                        serenity::model::channel::ChannelType::Forum => "forum",
                        serenity::model::channel::ChannelType::Stage => "stage",
                        _ => "text",
                    };
                    text.push_str(&format!("- #{} (id: {}, {})\n", ch.name, ch.id.get(), kind));
                }
            }
        }

        if let Ok(mut instructions) = self.inner.instructions.write() {
            *instructions = text;
        }
    }

    pub(crate) fn http(&self) -> &Arc<Http> {
        &self.inner.http
    }

    pub(crate) fn cache(&self) -> Option<&Arc<Cache>> {
        self.inner.cache.get()
    }
}
