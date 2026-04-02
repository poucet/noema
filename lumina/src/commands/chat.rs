//! /chat — chat channel management.

use lumina_macros::command_group;
use serenity::all::{
    ChannelId, CommandInteraction, CreateInteractionResponse, CreateInteractionResponseMessage,
    EditChannel, PermissionOverwrite, PermissionOverwriteType, Permissions,
};
use serenity::model::channel::ChannelType;

use super::LuminaContext;

#[command_group(description = "Chat management")]
mod chat {
    use super::*;

    #[sub_command(description = "Create a new AI chat channel")]
    pub async fn new(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("Channel name (defaults to your username)")] name: Option<String>,
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
            let response = CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(format!("Channel `{channel_name}` already exists."))
                    .ephemeral(true),
            );
            cmd.create_response(&lx.http, response).await?;
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
            .say(&lx.http, format!("Chat channel for <@{}>. Say anything and I'll respond!", cmd.user.id))
            .await?;

        let response = CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content(format!("Created <#{}>", channel.id))
                .ephemeral(true),
        );
        cmd.create_response(&lx.http, response).await?;

        Ok(())
    }

    #[sub_command(description = "Pause bot responses in this channel")]
    pub async fn pause(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
    ) -> anyhow::Result<()> {
        let channel_id = cmd.channel_id;
        let mut paused = lx.state.paused_channels.write().await;
        if !paused.insert(channel_id) {
            reply_ephemeral(lx, cmd, "Already paused.").await?;
        } else {
            reply_ephemeral(lx, cmd, "Paused. Use `/chat resume` to resume.").await?;
        }
        Ok(())
    }

    #[sub_command(description = "Resume bot responses in this channel")]
    pub async fn resume(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
    ) -> anyhow::Result<()> {
        let channel_id = cmd.channel_id;
        let mut paused = lx.state.paused_channels.write().await;
        if paused.remove(&channel_id) {
            reply_ephemeral(lx, cmd, "Resumed.").await?;
        } else {
            reply_ephemeral(lx, cmd, "Not paused.").await?;
        }
        Ok(())
    }

    #[sub_command(description = "Set the LLM model")]
    pub async fn model(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("Model ID (e.g. claude-sonnet-4-20250514)")] model_id: Option<String>,
    ) -> anyhow::Result<()> {
        use simply_daemon::api::ModelApi;
        match model_id {
            Some(id) => {
                lx.daemon.set_default_model(&id).await?;
                reply_ephemeral(lx, cmd, &format!("Model set to `{id}`")).await?;
            }
            None => {
                let current = lx.daemon.default_model_id().await;
                reply_ephemeral(lx, cmd, &format!("Current model: `{current}`")).await?;
            }
        }
        Ok(())
    }

    async fn reply_ephemeral(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        content: &str,
    ) -> anyhow::Result<()> {
        let response = CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content(content)
                .ephemeral(true),
        );
        cmd.create_response(&lx.http, response).await?;
        Ok(())
    }
}
