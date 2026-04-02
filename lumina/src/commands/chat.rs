//! /chat — chat channel management.

use lumina_macros::command_group;
use serenity::all::{
    CommandInteraction, CreateInteractionResponse, CreateInteractionResponseMessage,
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

        let channel_name = name
            .unwrap_or_else(|| cmd.user.name.clone())
            .to_lowercase()
            .replace(' ', "-");

        // Find or create the "AI Chats" category
        let guild_channels = guild_id.channels(&lx.http).await?;
        let category = guild_channels
            .values()
            .find(|c| c.kind == ChannelType::Category && c.name == "AI Chats");

        let category_id = if let Some(cat) = category {
            // Check for duplicate channel name in the category
            let existing = guild_channels
                .values()
                .any(|c| c.parent_id == Some(cat.id) && c.name == channel_name);
            if existing {
                let response = CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content(format!("Channel `{channel_name}` already exists in AI Chats."))
                        .ephemeral(true),
                );
                cmd.create_response(&lx.http, response).await?;
                return Ok(());
            }
            cat.id
        } else {
            guild_id
                .create_channel(&lx.http, serenity::builder::CreateChannel::new("AI Chats").kind(ChannelType::Category))
                .await?
                .id
        };

        // Create the channel under the category
        let channel = guild_id
            .create_channel(
                &lx.http,
                serenity::builder::CreateChannel::new(&channel_name)
                    .kind(ChannelType::Text)
                    .category(category_id),
            )
            .await?;

        // Set permissions: deny @everyone, allow invoking user + bot
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

        // Welcome message
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
}
