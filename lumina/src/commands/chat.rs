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
        let partial = ac.data.autocomplete()
            .map(|opt| opt.value.to_string())
            .unwrap_or_default();
        let partial_lower = partial.to_lowercase();

        tracing::debug!(partial = %partial, "model autocomplete");

        let anon = simply_rpc::RequestContext::anonymous();
        let models = lx.daemon.model().list_models(&anon).await.unwrap_or_default();
        let choices: Vec<AutocompleteChoice> = models
            .into_iter()
            .filter(|m| m.definition.capabilities.contains(&simply_daemon::api::ModelCapability::Text))
            .filter(|m| {
                if partial_lower.is_empty() { return true; }
                let full_id = m.id.to_string().to_lowercase();
                let name = m.definition.name().to_lowercase();
                let display = m.definition.display_name.as_deref().unwrap_or("").to_lowercase();
                full_id.contains(&partial_lower)
                    || name.contains(&partial_lower)
                    || display.contains(&partial_lower)
            })
            .take(25) // Discord max autocomplete choices
            .map(|m| {
                let full_id = m.id.to_string();
                let label = match &m.definition.display_name {
                    Some(name) => format!("{name}  ({full_id})"),
                    None => full_id.clone(),
                };
                let label = if label.len() > 100 { label[..100].to_string() } else { label };
                AutocompleteChoice::new(label, full_id)
            })
            .collect();

        tracing::debug!(partial = %partial, choices = choices.len(), "model autocomplete results");

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
    if has_topic_tag(lx, cmd.channel_id, "paused") {
        return reply_ephemeral(lx, cmd, "Already paused.").await;
    }
    reply_ephemeral(lx, cmd, "Paused. Use `/chat resume` to resume.").await?;
    set_topic_tag(lx, cmd.channel_id, "paused", "true").await;
    Ok(())
}

async fn cmd_resume(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    if !has_topic_tag(lx, cmd.channel_id, "paused") {
        return reply_ephemeral(lx, cmd, "Not paused.").await;
    }
    reply_ephemeral(lx, cmd, "Resumed.").await?;
    remove_topic_tag(lx, cmd.channel_id, "paused").await;
    Ok(())
}

async fn cmd_model(
    lx: &LuminaContext,
    cmd: &CommandInteraction,
    model_id: Option<String>,
) -> anyhow::Result<()> {
    let channel_id = cmd.channel_id;
    match model_id {
        Some(id) => {
            // Validate the model exists and supports text/chat
            let anon = simply_rpc::RequestContext::anonymous();
            let models = lx.daemon.model().list_models(&anon).await.unwrap_or_default();
            let model = models.iter().find(|m| m.id.to_string() == id);
            match model {
                None => {
                    return reply_ephemeral(lx, cmd, &format!("Unknown model: `{id}`")).await;
                }
                Some(m) if !m.definition.capabilities.contains(&simply_daemon::api::ModelCapability::Text) => {
                    return reply_ephemeral(lx, cmd, &format!("`{id}` does not support chat (capabilities: {:?})", m.definition.capabilities)).await;
                }
                _ => {}
            }
            reply_ephemeral(lx, cmd, &format!("Model for this channel set to `{id}`")).await?;
            set_topic_tag(lx, channel_id, "model", &id).await;
            Ok(())
        }
        None => {
            let current = get_topic_tag(lx, channel_id, "model");
            let default = lx
                .config
                .discord
                .model_id
                .as_deref()
                .filter(|s| !s.is_empty());
            let display = current
                .as_deref()
                .or(default)
                .unwrap_or("(daemon default)");
            reply_ephemeral(lx, cmd, &format!("Current model: `{display}`")).await
        }
    }
}

// ---------------------------------------------------------------------------
// Channel topic tag helpers
// ---------------------------------------------------------------------------

/// Set a `[key:value]` tag in the channel topic. Replaces existing tag if present.
async fn set_topic_tag(lx: &LuminaContext, channel_id: ChannelId, key: &str, value: &str) {
    let current_topic = get_channel_topic(lx, channel_id).unwrap_or_default();
    let new_topic = update_tag_in_topic(&current_topic, key, Some(value));
    tracing::debug!(channel = %channel_id, current = %current_topic, new = %new_topic, "setting topic tag");
    if let Err(e) = channel_id.edit(&lx.http, EditChannel::new().topic(&new_topic)).await {
        tracing::error!(channel = %channel_id, error = %e, "failed to set channel topic");
    }
}

/// Remove a `[key:...]` or `[key]` tag from the channel topic.
async fn remove_topic_tag(lx: &LuminaContext, channel_id: ChannelId, key: &str) {
    let current_topic = get_channel_topic(lx, channel_id).unwrap_or_default();
    let new_topic = update_tag_in_topic(&current_topic, key, None);
    let _ = channel_id
        .edit(&lx.http, EditChannel::new().topic(&new_topic))
        .await;
}

fn get_channel_topic(lx: &LuminaContext, channel_id: ChannelId) -> Option<String> {
    // Try each guild in cache to find the channel
    for guild_id in &lx.config.discord.guild_ids {
        if let Some(guild) = lx.ctx.cache.guild(serenity::model::id::GuildId::new(*guild_id)) {
            if let Some(ch) = guild.channels.get(&channel_id) {
                return ch.topic.clone();
            }
        }
    }
    None
}

/// Check if a tag exists in the channel topic.
pub fn has_topic_tag(lx: &LuminaContext, channel_id: ChannelId, key: &str) -> bool {
    get_topic_tag(lx, channel_id, key).is_some()
}

/// Get the value of a `[key:value]` tag from the channel topic, or empty string for bare `[key]`.
pub fn get_topic_tag(lx: &LuminaContext, channel_id: ChannelId, key: &str) -> Option<String> {
    let topic = get_channel_topic(lx, channel_id)?;
    let prefix = format!("[{key}:");
    let bare = format!("[{key}]");
    for word in topic.split_whitespace() {
        if word == bare {
            return Some(String::new());
        }
        if word.starts_with(&prefix) && word.ends_with(']') {
            return Some(word[prefix.len()..word.len() - 1].to_string());
        }
    }
    None
}

/// Update or remove a tag in topic text.
/// Tags are `[key:value]` or `[key]` at the start, separated by spaces.
fn update_tag_in_topic(topic: &str, key: &str, value: Option<&str>) -> String {
    let tag_prefix = format!("[{key}:");
    let tag_bare = format!("[{key}]");

    // Remove existing tag
    let cleaned: String = topic
        .split_whitespace()
        .filter(|word| !word.starts_with(&tag_prefix) && *word != tag_bare)
        .collect::<Vec<_>>()
        .join(" ");

    match value {
        Some(v) => {
            let new_tag = format!("[{key}:{v}]");
            if cleaned.is_empty() {
                new_tag
            } else {
                // Put tags at the front
                let (tags, rest) = split_tags_and_rest(&cleaned);
                let mut parts = tags;
                parts.push(new_tag);
                if !rest.is_empty() {
                    parts.push(rest);
                }
                parts.join(" ")
            }
        }
        None => cleaned,
    }
}

/// Split a topic into tag portion and the rest.
fn split_tags_and_rest(topic: &str) -> (Vec<String>, String) {
    let mut tags = Vec::new();
    let mut rest_start = 0;

    for word in topic.split_whitespace() {
        if word.starts_with('[') && word.ends_with(']') {
            tags.push(word.to_string());
            rest_start += word.len() + 1; // +1 for the space
        } else {
            break;
        }
    }

    let rest = if rest_start < topic.len() {
        topic[rest_start..].trim().to_string()
    } else {
        String::new()
    };

    (tags, rest)
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
