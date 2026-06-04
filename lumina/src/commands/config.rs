//! /config — set Lumina's per-server Discord settings.
//!
//! Uses native Discord channel pickers (no copy-pasting ids). The bot token is
//! deliberately NOT settable here — it has to bootstrap via the web setup
//! wizard, since the bot needs the token to be on Discord in the first place.

use async_trait::async_trait;
use serenity::all::{ChannelType, CommandInteraction, CommandOptionType, ResolvedValue};
use serenity::builder::{CreateCommand, CreateCommandOption};

use super::{LuminaContext, SlashCommand};
use crate::register_command;

#[derive(Default)]
pub struct Config;

#[async_trait]
impl SlashCommand for Config {
    fn name(&self) -> &'static str {
        "config"
    }

    fn register(&self) -> CreateCommand {
        CreateCommand::new("config")
            .description("Set Lumina's channel settings for this server")
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::Channel,
                    "status_channel",
                    "Channel for the bot's online/offline status messages",
                )
                .channel_types(vec![ChannelType::Text]),
            )
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::Channel,
                    "ai_category",
                    "Category where /chat new creates its channels",
                )
                .channel_types(vec![ChannelType::Category]),
            )
    }

    async fn run(&self, lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let mut status_channel = None;
        let mut ai_category = None;
        for o in cmd.data.options() {
            if let ResolvedValue::Channel(ch) = o.value {
                match o.name {
                    "status_channel" => status_channel = Some(ch.id.get()),
                    "ai_category" => ai_category = Some(ch.id.get()),
                    _ => {}
                }
            }
        }

        if status_channel.is_none() && ai_category.is_none() {
            return lx
                .reply_ephemeral(cmd, "Nothing to set — pass `status_channel` and/or `ai_category`.")
                .await;
        }

        // Persist to lumina.toml so the values survive a restart.
        let mut cfg = config::LuminaConfig::load();
        if let Some(id) = status_channel {
            cfg.discord.status_channel_id = Some(id);
        }
        if let Some(id) = ai_category {
            cfg.discord.ai_chats_category_id = Some(id);
        }
        if let Err(e) = cfg.save() {
            return lx.reply_ephemeral(cmd, &format!("Failed to save config: {e}")).await;
        }

        // Also update the live in-memory config so `/chat new` (which reads the
        // shared ConfigKey) picks up the AI category without a restart. This MUST
        // be deferred to a spawned task: the interaction dispatcher holds a READ
        // lock on ctx.data for the duration of this command, so write-locking
        // inline would deadlock (the command would hang and never respond). The
        // spawned write runs once dispatch releases the read lock.
        let ctx = lx.ctx.clone();
        tokio::spawn(async move {
            let mut data = ctx.data.write().await;
            if let Some(c) = data.get_mut::<crate::ConfigKey>() {
                if let Some(id) = status_channel {
                    c.discord.status_channel_id = Some(id);
                }
                if let Some(id) = ai_category {
                    c.discord.ai_chats_category_id = Some(id);
                }
            }
        });

        let mut msg = String::from("✅ Saved:");
        if let Some(id) = ai_category {
            msg.push_str(&format!("\n• AI category → <#{id}> (active now)"));
        }
        if let Some(id) = status_channel {
            msg.push_str(&format!("\n• Status channel → <#{id}> (active after next restart)"));
        }
        lx.reply_ephemeral(cmd, &msg).await
    }
}

register_command!(Config);
