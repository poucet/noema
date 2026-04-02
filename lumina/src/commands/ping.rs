//! /ping — health check command.

use lumina_macros::slash_command;
use serenity::all::CommandInteraction;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};

use super::LuminaContext;

#[slash_command(description = "Check if Lumina is alive")]
async fn ping(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().content("Pong!"),
    );
    cmd.create_response(&lx.http, response).await?;
    Ok(())
}
