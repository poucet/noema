//! /invite — get the OAuth URL for adding Lumina to another server.
//!
//! The URL's permission set is computed from what Lumina actually
//! uses (see [`crate::invite::required_permissions`]); this command
//! just fetches the bot's application id from Discord at runtime and
//! hands the user the link. Replies ephemerally so it doesn't clutter
//! the channel — invite links are for the invoker, not the whole
//! guild.

use lumina_macros::slash_command;
use serenity::all::CommandInteraction;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};

use super::LuminaContext;

#[slash_command(description = "Get a Discord invite URL to add Lumina to another server")]
async fn invite(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let app_info = lx.http.get_current_application_info().await?;
    let url = crate::invite::invite_url(app_info.id.get());
    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .ephemeral(true)
            .content(format!(
                "Add Lumina to a server — you need **Manage Server** on that guild:\n\n{url}"
            )),
    );
    cmd.create_response(&lx.http, response).await?;
    Ok(())
}
