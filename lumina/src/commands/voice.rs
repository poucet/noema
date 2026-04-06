//! /voice — Discord voice channel commands.
//!
//! Thin command handlers that delegate to the voice module.

use async_trait::async_trait;
use serenity::all::{
    CommandInteraction, CommandOptionType, CreateInteractionResponse,
    CreateInteractionResponseMessage, ResolvedValue,
};
use serenity::builder::{CreateCommand, CreateCommandOption};

use super::LuminaContext;
use crate::register_command;
use crate::voice::{VoiceManagerKey, VoiceMode};

#[derive(Default)]
pub struct Voice;

register_command!(Voice);

#[async_trait]
impl super::SlashCommand for Voice {
    fn name(&self) -> &'static str {
        "voice"
    }

    fn register(&self) -> CreateCommand {
        CreateCommand::new("voice")
            .description("Voice channel commands")
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "transcribe",
                    "Join your voice channel and transcribe speech to this text channel",
                ),
            )
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "listen",
                    "Join your voice channel for a voice conversation",
                ),
            )
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "say",
                    "Speak text in the voice channel via TTS",
                )
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "text", "Text to speak")
                        .required(true),
                ),
            )
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "leave",
                    "Leave the voice channel",
                ),
            )
    }

    async fn run(&self, lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let options = cmd.data.options();
        let subcommand = options.first().map(|o| o.name).unwrap_or("");

        match subcommand {
            "transcribe" => cmd_transcribe(lx, cmd).await,
            "listen" => cmd_listen(lx, cmd).await,
            "say" => {
                let text = options.first()
                    .and_then(|o| match &o.value {
                        ResolvedValue::SubCommand(opts) => opts.first(),
                        _ => None,
                    })
                    .and_then(|o| match &o.value {
                        ResolvedValue::String(s) => Some(*s),
                        _ => None,
                    })
                    .unwrap_or("");
                cmd_say(lx, cmd, text).await
            }
            "leave" => cmd_leave(lx, cmd).await,
            _ => reply(lx, cmd, "Unknown subcommand").await,
        }
    }
}

async fn cmd_transcribe(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let (guild_id, voice_channel) = match join_user_channel(lx, cmd).await {
        Ok(ids) => ids,
        Err(msg) => return reply(lx, cmd, &msg).await,
    };

    let voice_mgr = get_voice_manager(lx).await?;
    let text_channel = cmd.channel_id;
    voice_mgr.start_session(guild_id, voice_channel, text_channel, VoiceMode::Transcribe).await;

    // TODO: register songbird voice receive handler

    reply(lx, cmd, "Joined voice. Transcribing to this channel...").await
}

async fn cmd_listen(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let (guild_id, voice_channel) = match join_user_channel(lx, cmd).await {
        Ok(ids) => ids,
        Err(msg) => return reply(lx, cmd, &msg).await,
    };

    let voice_mgr = get_voice_manager(lx).await?;
    let text_channel = cmd.channel_id;
    voice_mgr.start_session(guild_id, voice_channel, text_channel, VoiceMode::Listen).await;

    // TODO: register songbird voice receive handler + session

    reply(lx, cmd, "Joined voice. Listening for conversation...").await
}

async fn cmd_say(lx: &LuminaContext, cmd: &CommandInteraction, text: &str) -> anyhow::Result<()> {
    if text.is_empty() {
        return reply(lx, cmd, "No text provided").await;
    }

    let guild_id = cmd.guild_id.ok_or_else(|| anyhow::anyhow!("Not in a guild"))?;
    let manager = songbird::get(&lx.ctx).await
        .ok_or_else(|| anyhow::anyhow!("Songbird not initialized"))?;

    let handler_lock = match manager.get(guild_id) {
        Some(h) => h,
        None => return reply(lx, cmd, "Not in a voice channel. Use `/voice transcribe` or `/voice listen` first.").await,
    };

    reply(lx, cmd, &format!("Speaking: _{text}_")).await?;

    let voice_mgr = get_voice_manager(lx).await?;
    let stereo_samples = voice_mgr.synthesize_for_discord(text).await
        .map_err(|e| anyhow::anyhow!("TTS failed: {e}"))?;

    // Play via songbird
    let bytes: Vec<u8> = stereo_samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let input = songbird::input::RawAdapter::new(
        std::io::Cursor::new(bytes),
        48_000,
        2,
    );
    let mut handler = handler_lock.lock().await;
    handler.play_input(input.into());

    Ok(())
}

async fn cmd_leave(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let guild_id = cmd.guild_id.ok_or_else(|| anyhow::anyhow!("Not in a guild"))?;
    let manager = songbird::get(&lx.ctx).await
        .ok_or_else(|| anyhow::anyhow!("Songbird not initialized"))?;

    // Clean up session
    if let Ok(voice_mgr) = get_voice_manager(lx).await {
        voice_mgr.stop_session(&guild_id).await;
    }

    manager.leave(guild_id).await?;
    reply(lx, cmd, "Left voice channel.").await
}

// --- Helpers ---

async fn get_voice_manager(lx: &LuminaContext) -> anyhow::Result<std::sync::Arc<crate::voice::VoiceManager>> {
    let data = lx.ctx.data.read().await;
    data.get::<VoiceManagerKey>()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("VoiceManager not initialized"))
}

async fn join_user_channel(
    lx: &LuminaContext,
    cmd: &CommandInteraction,
) -> Result<(serenity::model::id::GuildId, serenity::model::id::ChannelId), String> {
    let guild_id = cmd.guild_id.ok_or("Not in a guild")?;

    let voice_channel = {
        let guild = lx.cache.guild(guild_id).ok_or("Guild not found")?;
        guild.voice_states.get(&cmd.user.id)
            .and_then(|vs| vs.channel_id)
            .ok_or("You're not in a voice channel")?
    };

    let manager = songbird::get(&lx.ctx).await
        .ok_or("Songbird not initialized")?;

    manager.join(guild_id, voice_channel).await
        .map_err(|e| format!("Failed to join voice: {e}"))?;

    tracing::info!(guild_id = %guild_id, voice_channel = %voice_channel, "joined voice channel");
    Ok((guild_id, voice_channel))
}

async fn reply(lx: &LuminaContext, cmd: &CommandInteraction, msg: &str) -> anyhow::Result<()> {
    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().content(msg),
    );
    cmd.create_response(&lx.http, response).await?;
    Ok(())
}
