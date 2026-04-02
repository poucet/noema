//! /ping — health check command.

use lumina_macros::slash_command;
use serenity::all::CommandInteraction;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::prelude::*;

#[slash_command(description = "Check if Lumina is alive")]
async fn ping(ctx: &Context, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().content("Pong!"),
    );
    cmd.create_response(&ctx.http, response).await?;
    Ok(())
}
