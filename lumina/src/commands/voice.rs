//! /voice — Discord voice channel commands.

use std::collections::HashMap;
use std::sync::Arc;

use serenity::all::{
    AutocompleteChoice, ChannelId, CommandInteraction, CreateAutocompleteResponse,
    CreateInteractionResponse, CreateInteractionResponseMessage, ResolvedOption, ResolvedValue,
};
use serenity::builder::GetMessages;
use simply_daemon::api::*;

use super::LuminaContext;
use crate::voice::{VoiceManagerKey, VoiceMode};

#[lumina_macros::command_group(description = "Voice channel commands")]
mod voice {
    use super::*;

    #[sub_command(description = "Join your voice channel and transcribe speech")]
    pub async fn transcribe(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let (guild_id, voice_channel) = join_user_channel(lx, cmd).await
            .map_err(|e| anyhow::anyhow!(e))?;

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

        lx.reply(cmd, "Joined voice. Transcribing to this channel...").await
    }

    #[sub_command(description = "Join your voice channel for a voice conversation")]
    pub async fn listen(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let (guild_id, voice_channel) = join_user_channel(lx, cmd).await
            .map_err(|e| anyhow::anyhow!(e))?;

        let voice_mgr = get_voice_manager(lx).await?;
        let text_channel = cmd.channel_id;

        let history = load_channel_seed(lx, text_channel).await;

        let system_prompt = format!(
            "You are Lumina, an AI assistant in a live voice conversation on Discord.\n\
             Channel: <#{}>\nGuild: {}\n\n\
             Keep responses concise — they will be spoken aloud via TTS.",
            text_channel, guild_id,
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

        lx.reply(cmd, "Joined voice. Listening for conversation...").await
    }

    #[sub_command(description = "Speak text in the voice channel via TTS")]
    pub async fn say(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("Text to speak")] text: String,
    ) -> anyhow::Result<()> {
        if text.is_empty() {
            return lx.reply(cmd, "No text provided").await;
        }

        let guild_id = cmd.guild_id.ok_or_else(|| anyhow::anyhow!("Not in a guild"))?;
        let manager = songbird::get(&lx.ctx).await
            .ok_or_else(|| anyhow::anyhow!("Songbird not initialized"))?;

        // Auto-join if not connected
        let handler_lock = match manager.get(guild_id) {
            Some(h) => h,
            None => {
                join_user_channel(lx, cmd).await.map_err(|e| anyhow::anyhow!(e))?;
                manager.get(guild_id).ok_or_else(|| anyhow::anyhow!("Failed to join"))?
            }
        };

        lx.reply(cmd, &format!("Speaking: _{text}_")).await?;

        let voice_mgr = get_voice_manager(lx).await?;
        let stereo = voice_mgr.synthesize_for_discord(&text).await?;
        let bytes: Vec<u8> = stereo.iter().flat_map(|s| s.to_le_bytes()).collect();
        let input = songbird::input::RawAdapter::new(std::io::Cursor::new(bytes), 48_000, 2);
        let mut handler = handler_lock.lock().await;
        handler.play_input(input.into());

        Ok(())
    }

    #[sub_command(description = "Leave the voice channel")]
    pub async fn leave(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let guild_id = cmd.guild_id.ok_or_else(|| anyhow::anyhow!("Not in a guild"))?;
        let manager = songbird::get(&lx.ctx).await
            .ok_or_else(|| anyhow::anyhow!("Songbird not initialized"))?;

        if let Ok(voice_mgr) = get_voice_manager(lx).await {
            voice_mgr.stop_session(&guild_id).await;
        }

        manager.leave(guild_id).await?;
        lx.reply(cmd, "Left voice channel.").await
    }

    #[sub_command(description = "Set STT or TTS provider")]
    pub async fn provider(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("stt or tts")] r#type: String,
        #[describe("Provider ID")] #[autocomplete] id: String,
    ) -> anyhow::Result<()> {
        let voice_mgr = get_voice_manager(lx).await?;
        match r#type.as_str() {
            "stt" => {
                voice_mgr.set_stt_provider(id.clone()).await;
                lx.reply(cmd, &format!("STT provider set to **{id}**")).await
            }
            "tts" => {
                voice_mgr.set_tts_provider(id.clone()).await;
                lx.reply(cmd, &format!("TTS provider set to **{id}**")).await
            }
            _ => lx.reply(cmd, "Type must be 'stt' or 'tts'").await,
        }
    }

    #[sub_command(description = "Set the TTS voice")]
    pub async fn set_voice(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("Voice ID")] #[autocomplete] id: String,
    ) -> anyhow::Result<()> {
        let voice_mgr = get_voice_manager(lx).await?;
        voice_mgr.set_tts_voice(id.clone()).await;
        lx.reply(cmd, &format!("TTS voice set to **{id}**")).await
    }

    #[sub_command(description = "List available providers and voices")]
    pub async fn list(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let voice_mgr = get_voice_manager(lx).await?;
        let providers = voice_mgr.daemon().voice().list_voice_providers().await?;

        let mut lines = vec!["**Providers:**".to_string()];
        for p in &providers {
            lines.push(format!("  `{}` — {} ({})", p.id, p.name, p.capabilities.join(", ")));
        }

        for p in providers.iter().filter(|p| p.capabilities.contains(&"tts".to_string())) {
            if let Ok(voices) = voice_mgr.daemon().voice().list_voices(&p.id).await {
                if !voices.is_empty() {
                    lines.push(format!("\n**Voices for {}:**", p.id));
                    for v in voices.iter().take(15) {
                        lines.push(format!("  `{}` — {}", v.id, v.name));
                    }
                    if voices.len() > 15 {
                        lines.push(format!("  ... and {} more", voices.len() - 15));
                    }
                }
            }
        }

        lx.reply(cmd, &lines.join("\n")).await
    }

    #[sub_command(description = "Show current voice settings")]
    pub async fn status(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
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

        lx.reply(cmd, &format!(
            "**Voice Settings**\n\
             STT Provider: `{stt}`\n\
             TTS Provider: `{tts}`\n\
             TTS Voice: `{voice}`\n\
             Session: {mode_str}"
        )).await
    }

    /// Autocomplete handler — called by the macro-generated `autocomplete` method.
    pub async fn autocomplete(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let voice_mgr = get_voice_manager(lx).await?;
        let options = cmd.data.options();
        let subcommand = options.first().map(|o| o.name).unwrap_or("");

        let choices: Vec<AutocompleteChoice> = match subcommand {
            "provider" => {
                let providers = voice_mgr.daemon().voice().list_voice_providers().await.unwrap_or_default();
                providers.iter()
                    .map(|p| AutocompleteChoice::new(
                        format!("{} ({})", p.name, p.capabilities.join(", ")),
                        p.id.clone(),
                    ))
                    .collect()
            }
            "set-voice" => {
                let tts_id = voice_mgr.tts_provider_id().await.unwrap_or_default();
                let voices = voice_mgr.daemon().voice().list_voices(&tts_id).await.unwrap_or_default();
                voices.iter()
                    .map(|v| AutocompleteChoice::new(&v.name, v.id.clone()))
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

// --- Helpers (outside the module, used by subcommands via super::) ---

async fn get_voice_manager(lx: &LuminaContext) -> anyhow::Result<Arc<crate::voice::VoiceManager>> {
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
    let manager = songbird::get(&lx.ctx).await.ok_or("Songbird not initialized")?;
    manager.join(guild_id, voice_channel).await
        .map_err(|e| format!("Failed to join voice: {e}"))?;
    tracing::info!(guild_id = %guild_id, voice_channel = %voice_channel, "joined voice channel");
    Ok((guild_id, voice_channel))
}

async fn load_channel_seed(lx: &LuminaContext, channel_id: ChannelId) -> Vec<SeedMessage> {
    let bot_id = lx.cache.current_user().id.get();
    let messages = channel_id
        .messages(&lx.http, GetMessages::new().limit(50))
        .await
        .unwrap_or_default();

    messages.iter().rev()
        .filter(|m| !m.content.is_empty())
        .map(|m| {
            let role = if m.author.id.get() == bot_id { Role::Assistant } else { Role::User };
            let text = if role == Role::User {
                format!("<@{}> says: {}", m.author.id, m.content)
            } else {
                m.content.clone()
            };
            SeedMessage { role, content: vec![InputContent::Text { text }] }
        })
        .collect()
}
