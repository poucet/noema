//! /chat — chat channel management with subcommands.

use async_trait::async_trait;
use serenity::all::{
    AutocompleteChoice, ChannelId, CommandInteraction, CommandOptionType,
    CreateAutocompleteResponse, CreateInteractionResponse, CreateInteractionResponseMessage,
    EditChannel, PermissionOverwrite, PermissionOverwriteType, Permissions, ResolvedOption,
    ResolvedValue,
};
use serenity::builder::{CreateCommand, CreateCommandOption};
use serenity::model::channel::ChannelType;
use simply_daemon::api::ModelApi;

use super::LuminaContext;
use crate::register_command;

#[derive(Default)]
pub struct Chat;

register_command!(Chat);

#[async_trait]
impl super::SlashCommand for Chat {
    fn name(&self) -> &'static str {
        "chat"
    }

    fn register(&self) -> CreateCommand {
        CreateCommand::new("chat")
            .description("Chat management")
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "new", "Create a new AI chat channel")
                    .add_sub_option(
                        CreateCommandOption::new(CommandOptionType::String, "name", "Channel name (defaults to your username)"),
                    ),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "pause", "Pause bot responses in this channel"),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "resume", "Resume bot responses in this channel"),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "model", "Set the LLM model for this channel")
                    .add_sub_option(
                        CreateCommandOption::new(CommandOptionType::String, "model_id", "Model ID")
                            .set_autocomplete(true),
                    ),
            )
    }

    async fn run(&self, lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let opts = cmd.data.options();
        let sub = opts.first().ok_or_else(|| anyhow::anyhow!("missing subcommand"))?;

        match sub.name {
            "new" => {
                let name = if let ResolvedValue::SubCommand(ref sub_opts) = sub.value {
                    sub_opts.iter().find_map(|o| match o {
                        ResolvedOption { name: "name", value: ResolvedValue::String(s), .. } => Some(s.to_string()),
                        _ => None,
                    })
                } else {
                    None
                };
                cmd_new(lx, cmd, name).await
            }
            "pause" => cmd_pause(lx, cmd).await,
            "resume" => cmd_resume(lx, cmd).await,
            "model" => {
                let model_id = if let ResolvedValue::SubCommand(ref sub_opts) = sub.value {
                    sub_opts.iter().find_map(|o| match o {
                        ResolvedOption { name: "model_id", value: ResolvedValue::String(s), .. } => Some(s.to_string()),
                        _ => None,
                    })
                } else {
                    None
                };
                cmd_model(lx, cmd, model_id).await
            }
            other => Err(anyhow::anyhow!("unknown subcommand `{other}`")),
        }
    }

    async fn autocomplete(&self, lx: &LuminaContext, ac: &CommandInteraction) -> anyhow::Result<()> {
        // Find the focused option — it's nested inside the subcommand
        let opts = ac.data.options();
        let sub = match opts.first() {
            Some(o) => o,
            None => return Ok(()),
        };

        if sub.name != "model" {
            return Ok(());
        }

        let partial = if let ResolvedValue::SubCommand(ref sub_opts) = sub.value {
            sub_opts.iter().find_map(|o| match o {
                ResolvedOption { name: "model_id", value: ResolvedValue::String(s), .. } => Some(s.to_string()),
                _ => None,
            }).unwrap_or_default()
        } else {
            String::new()
        };

        let partial_lower = partial.to_lowercase();

        // Fetch models from daemon and filter by partial match
        let models = lx.daemon.list_models().await.unwrap_or_default();
        let choices: Vec<AutocompleteChoice> = models
            .into_iter()
            .filter(|m| {
                let id = m.id.to_string().to_lowercase();
                let name = m.definition.name().to_lowercase();
                partial_lower.is_empty() || id.contains(&partial_lower) || name.contains(&partial_lower)
            })
            .take(25) // Discord max autocomplete choices
            .map(|m| {
                let display = m.definition.display_name
                    .as_deref()
                    .unwrap_or(&m.id.model);
                AutocompleteChoice::new(display.to_string(), m.id.to_string())
            })
            .collect();

        ac.create_response(
            &lx.http,
            CreateInteractionResponse::Autocomplete(
                CreateAutocompleteResponse::new().set_choices(choices),
            ),
        )
        .await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Subcommand implementations
// ---------------------------------------------------------------------------

async fn cmd_new(
    lx: &LuminaContext,
    cmd: &CommandInteraction,
    name: Option<String>,
) -> anyhow::Result<()> {
    let guild_id = cmd
        .guild_id
        .ok_or_else(|| anyhow::anyhow!("must be used in a server"))?;

    let category_id = lx
        .config
        .discord
        .ai_chats_category_id
        .map(ChannelId::new)
        .ok_or_else(|| anyhow::anyhow!("ai_chats_category_id not set in lumina.toml"))?;

    let channel_name = name
        .unwrap_or_else(|| cmd.user.name.clone())
        .to_lowercase()
        .replace(' ', "-");

    let guild_channels = guild_id.channels(&lx.http).await?;
    let existing = guild_channels
        .values()
        .any(|c| c.parent_id == Some(category_id) && c.name == channel_name);
    if existing {
        reply_ephemeral(lx, cmd, &format!("Channel `{channel_name}` already exists.")).await?;
        return Ok(());
    }

    let channel = guild_id
        .create_channel(
            &lx.http,
            serenity::builder::CreateChannel::new(&channel_name)
                .kind(ChannelType::Text)
                .category(category_id),
        )
        .await?;

    let bot_id = lx.ctx.cache.current_user().id;
    let everyone_role = guild_id.everyone_role();

    let overwrites = vec![
        PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
            kind: PermissionOverwriteType::Role(everyone_role),
        },
        PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(cmd.user.id),
        },
        PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(bot_id),
        },
    ];

    channel
        .id
        .edit(&lx.http, EditChannel::new().permissions(overwrites))
        .await?;

    channel
        .id
        .say(
            &lx.http,
            format!(
                "Chat channel for <@{}>. Say anything and I'll respond!",
                cmd.user.id
            ),
        )
        .await?;

    reply_ephemeral(lx, cmd, &format!("Created <#{}>", channel.id)).await
}

async fn cmd_pause(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let mut paused = lx.state.paused_channels.write().await;
    if !paused.insert(cmd.channel_id) {
        reply_ephemeral(lx, cmd, "Already paused.").await
    } else {
        reply_ephemeral(lx, cmd, "Paused. Use `/chat resume` to resume.").await
    }
}

async fn cmd_resume(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let mut paused = lx.state.paused_channels.write().await;
    if paused.remove(&cmd.channel_id) {
        reply_ephemeral(lx, cmd, "Resumed.").await
    } else {
        reply_ephemeral(lx, cmd, "Not paused.").await
    }
}

async fn cmd_model(
    lx: &LuminaContext,
    cmd: &CommandInteraction,
    model_id: Option<String>,
) -> anyhow::Result<()> {
    let channel_id = cmd.channel_id;
    match model_id {
        Some(id) => {
            lx.state
                .channel_models
                .write()
                .await
                .insert(channel_id, id.clone());
            reply_ephemeral(lx, cmd, &format!("Model for this channel set to `{id}`")).await
        }
        None => {
            let models = lx.state.channel_models.read().await;
            let current = models.get(&channel_id);
            let default = lx
                .config
                .discord
                .model_id
                .as_deref()
                .filter(|s| !s.is_empty());
            let display = current
                .map(|s| s.as_str())
                .or(default)
                .unwrap_or("(daemon default)");
            reply_ephemeral(lx, cmd, &format!("Current model: `{display}`")).await
        }
    }
}

async fn reply_ephemeral(
    lx: &LuminaContext,
    cmd: &CommandInteraction,
    content: &str,
) -> anyhow::Result<()> {
    cmd.create_response(
        &lx.http,
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content(content)
                .ephemeral(true),
        ),
    )
    .await?;
    Ok(())
}
