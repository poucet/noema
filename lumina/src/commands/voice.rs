//! /voice — Discord voice channel commands.
//!
//! Thin command handlers that delegate to the voice module.

use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use serenity::all::{
    ChannelId, CommandInteraction, CommandOptionType, CreateInteractionResponse,
    CreateInteractionResponseMessage, ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateCommand, CreateCommandOption, GetMessages};

use simply_daemon::api::*;

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
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "provider",
                    "Set STT or TTS provider",
                )
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "type", "stt or tts")
                        .required(true)
                        .add_string_choice("STT (speech-to-text)", "stt")
                        .add_string_choice("TTS (text-to-speech)", "tts"),
                )
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "id", "Provider ID")
                        .required(true)
                        .set_autocomplete(true),
                ),
            )
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "voice",
                    "Set the TTS voice",
                )
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "id", "Voice ID")
                        .required(true)
                        .set_autocomplete(true),
                ),
            )
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "list",
                    "List available providers and voices",
                ),
            )
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "status",
                    "Show current voice settings",
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
            "provider" => {
                let sub_opts = get_sub_opts(&options);
                let ptype = sub_opts.get("type").unwrap_or(&"");
                let id = sub_opts.get("id").unwrap_or(&"");
                cmd_provider(lx, cmd, ptype, id).await
            }
            "voice" => {
                let sub_opts = get_sub_opts(&options);
                let id = sub_opts.get("id").unwrap_or(&"");
                cmd_voice(lx, cmd, id).await
            }
            "list" => cmd_list(lx, cmd).await,
            "status" => cmd_status(lx, cmd).await,
            _ => reply(lx, cmd, "Unknown subcommand").await,
        }
    }

    async fn autocomplete(&self, lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        use serenity::all::CreateAutocompleteResponse;

        let voice_mgr = get_voice_manager(lx).await?;
        let options = cmd.data.options();
        let subcommand = options.first().map(|o| o.name).unwrap_or("");

        let choices: Vec<_> = match subcommand {
            "provider" => {
                let providers = voice_mgr.daemon().voice().list_voice_providers().await.unwrap_or_default();
                let sub_opts = get_sub_opts(&options);
                let ptype = *sub_opts.get("type").unwrap_or(&"");
                providers.iter()
                    .filter(|p| match ptype {
                        "stt" => p.capabilities.contains(&"stt".to_string()),
                        "tts" => p.capabilities.contains(&"tts".to_string()),
                        _ => true,
                    })
                    .map(|p| serenity::all::AutocompleteChoice::new(
                        format!("{} ({})", p.name, p.capabilities.join(", ")),
                        p.id.clone(),
                    ))
                    .collect()
            }
            "voice" => {
                let tts_id = voice_mgr.tts_provider_id().await.unwrap_or_default();
                let voices = voice_mgr.daemon().voice().list_voices(&tts_id).await.unwrap_or_default();
                voices.iter()
                    .map(|v| serenity::all::AutocompleteChoice::new(&v.name, v.id.clone()))
                    .take(25)
                    .collect()
            }
            _ => vec![],
        };

        cmd.create_response(&lx.http, CreateInteractionResponse::Autocomplete(
            CreateAutocompleteResponse::new().set_choices(choices),
        )).await?;
        Ok(())
    }
}

async fn cmd_transcribe(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let (guild_id, voice_channel) = match join_user_channel(lx, cmd).await {
        Ok(ids) => ids,
        Err(msg) => return reply(lx, cmd, &msg).await,
    };

    let voice_mgr = get_voice_manager(lx).await?;
    let text_channel = cmd.channel_id;

    let manager = songbird::get(&lx.ctx).await
        .ok_or_else(|| anyhow::anyhow!("Songbird not initialized"))?;
    let call = manager.get(guild_id)
        .ok_or_else(|| anyhow::anyhow!("Not in voice channel"))?;

    voice_mgr.start_session(
        guild_id, voice_channel, text_channel,
        VoiceMode::Transcribe, None,
        call, Arc::clone(&lx.http),
    ).await?;

    reply(lx, cmd, "Joined voice. Transcribing to this channel...").await
}

async fn cmd_listen(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let (guild_id, voice_channel) = match join_user_channel(lx, cmd).await {
        Ok(ids) => ids,
        Err(msg) => return reply(lx, cmd, &msg).await,
    };

    let voice_mgr = get_voice_manager(lx).await?;
    let text_channel = cmd.channel_id;

    // Load channel history to seed the voice conversation session
    let history = load_channel_seed(lx, text_channel).await;

    let system_prompt = format!(
        "You are Lumina, an AI assistant in a live voice conversation on Discord.\n\
         Channel: <#{}>\nGuild: {}\n\n\
         Keep responses concise — they will be spoken aloud via TTS.\n\
         The conversation history below is from the text channel where the voice session started.",
        text_channel,
        guild_id,
    );

    let session = simply_daemon::DaemonSession::create(
        voice_mgr.daemon().clone(),
        CreateSessionOptions {
            persistence: Some(Persistence::Ephemeral),
            system_prompt: Some(system_prompt),
            model_id: None,
            seed: history,
        },
    ).await?;

    tracing::info!(session_id = %session.id(), "voice listen session created");

    let manager = songbird::get(&lx.ctx).await
        .ok_or_else(|| anyhow::anyhow!("Songbird not initialized"))?;
    let call = manager.get(guild_id)
        .ok_or_else(|| anyhow::anyhow!("Not in voice channel"))?;

    voice_mgr.start_session(
        guild_id, voice_channel, text_channel,
        VoiceMode::Listen, Some(session),
        call, Arc::clone(&lx.http),
    ).await?;

    reply(lx, cmd, "Joined voice. Listening for conversation...").await
}

/// Load recent channel messages as seed for the voice conversation.
async fn load_channel_seed(lx: &LuminaContext, channel_id: ChannelId) -> Vec<SeedMessage> {
    let bot_id = lx.cache.current_user().id.get();
    let messages = channel_id
        .messages(&lx.http, GetMessages::new().limit(50))
        .await
        .unwrap_or_default();

    messages.iter().rev()
        .filter(|m| !m.content.is_empty())
        .map(|m| {
            let role = if m.author.id.get() == bot_id {
                Role::Assistant
            } else {
                Role::User
            };
            let text = if role == Role::User {
                format!("<@{}> says: {}", m.author.id, m.content)
            } else {
                m.content.clone()
            };
            SeedMessage {
                role,
                content: vec![InputContent::Text { text }],
            }
        })
        .collect()
}

async fn cmd_say(lx: &LuminaContext, cmd: &CommandInteraction, text: &str) -> anyhow::Result<()> {
    if text.is_empty() {
        return reply(lx, cmd, "No text provided").await;
    }

    // Auto-join if not already in a voice channel
    let guild_id = cmd.guild_id.ok_or_else(|| anyhow::anyhow!("Not in a guild"))?;
    let manager = songbird::get(&lx.ctx).await
        .ok_or_else(|| anyhow::anyhow!("Songbird not initialized"))?;

    let handler_lock = match manager.get(guild_id) {
        Some(h) => h,
        None => {
            match join_user_channel(lx, cmd).await {
                Ok(_) => manager.get(guild_id).ok_or_else(|| anyhow::anyhow!("Failed to get handler after join"))?,
                Err(msg) => return reply(lx, cmd, &msg).await,
            }
        }
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

async fn cmd_provider(lx: &LuminaContext, cmd: &CommandInteraction, ptype: &str, id: &str) -> anyhow::Result<()> {
    if id.is_empty() {
        // List providers
        let voice_mgr = get_voice_manager(lx).await?;
        let providers = voice_mgr.daemon().voice().list_voice_providers().await?;
        let list: Vec<String> = providers.iter()
            .map(|p| format!("- **{}** ({})", p.id, p.capabilities.join(", ")))
            .collect();
        return reply(lx, cmd, &format!("Available providers:\n{}", list.join("\n"))).await;
    }

    let voice_mgr = get_voice_manager(lx).await?;
    match ptype {
        "stt" => {
            voice_mgr.set_stt_provider(id.to_string()).await;
            reply(lx, cmd, &format!("STT provider set to **{id}**")).await
        }
        "tts" => {
            voice_mgr.set_tts_provider(id.to_string()).await;
            reply(lx, cmd, &format!("TTS provider set to **{id}**")).await
        }
        _ => reply(lx, cmd, "Type must be 'stt' or 'tts'").await,
    }
}

async fn cmd_voice(lx: &LuminaContext, cmd: &CommandInteraction, id: &str) -> anyhow::Result<()> {
    let voice_mgr = get_voice_manager(lx).await?;

    if id.is_empty() {
        // List voices
        let tts_id = voice_mgr.tts_provider_id().await.unwrap_or_default();
        if tts_id.is_empty() {
            return reply(lx, cmd, "No TTS provider set. Use `/voice provider tts <id>` first.").await;
        }
        let voices = voice_mgr.daemon().voice().list_voices(&tts_id).await?;
        let list: Vec<String> = voices.iter()
            .map(|v| format!("- **{}**: {}", v.id, v.name))
            .collect();
        return reply(lx, cmd, &format!("Voices for {tts_id}:\n{}", list.join("\n"))).await;
    }

    voice_mgr.set_tts_voice(id.to_string()).await;
    reply(lx, cmd, &format!("TTS voice set to **{id}**")).await
}

async fn cmd_list(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let voice_mgr = get_voice_manager(lx).await?;
    let providers = voice_mgr.daemon().voice().list_voice_providers().await?;

    let mut lines = vec!["**Providers:**".to_string()];
    for p in &providers {
        let caps = p.capabilities.join(", ");
        lines.push(format!("  `{}` — {} ({})", p.id, p.name, caps));
    }

    // List voices for TTS providers
    for p in providers.iter().filter(|p| p.capabilities.contains(&"tts".to_string())) {
        match voice_mgr.daemon().voice().list_voices(&p.id).await {
            Ok(voices) if !voices.is_empty() => {
                lines.push(format!("\n**Voices for {}:**", p.id));
                for v in voices.iter().take(15) {
                    lines.push(format!("  `{}` — {}", v.id, v.name));
                }
                if voices.len() > 15 {
                    lines.push(format!("  ... and {} more", voices.len() - 15));
                }
            }
            _ => {}
        }
    }

    reply(lx, cmd, &lines.join("\n")).await
}

async fn cmd_status(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let voice_mgr = get_voice_manager(lx).await?;
    let stt = voice_mgr.stt_provider_id().await.unwrap_or_else(|| "(auto)".to_string());
    let tts = voice_mgr.tts_provider_id().await.unwrap_or_else(|| "(none)".to_string());
    let voice = voice_mgr.tts_voice_id().await.unwrap_or_else(|| "(auto)".to_string());

    let guild_id = cmd.guild_id.unwrap_or_default();
    let mode = voice_mgr.get_mode(&guild_id).await;
    let mode_str = match mode {
        Some(VoiceMode::Transcribe) => "Transcribing",
        Some(VoiceMode::Listen) => "Listening",
        None => "Not active",
    };

    reply(lx, cmd, &format!(
        "**Voice Settings**\n\
         STT Provider: `{stt}`\n\
         TTS Provider: `{tts}`\n\
         TTS Voice: `{voice}`\n\
         Session: {mode_str}"
    )).await
}

// --- Helpers ---

fn get_sub_opts<'a>(options: &'a [ResolvedOption<'a>]) -> HashMap<&'a str, &'a str> {
    let mut map = HashMap::new();
    if let Some(sub) = options.first() {
        if let ResolvedValue::SubCommand(opts) = &sub.value {
            for opt in opts {
                if let ResolvedValue::String(s) = &opt.value {
                    map.insert(opt.name, *s);
                }
            }
        }
    }
    map
}

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
