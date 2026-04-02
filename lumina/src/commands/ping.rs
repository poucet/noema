//! /ping — health check command.

use serenity::all::CommandInteraction;
use serenity::builder::{CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::prelude::*;

pub fn register() -> CreateCommand {
    CreateCommand::new("ping").description("Check if Lumina is alive")
}

pub async fn run(ctx: &Context, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().content("Pong!"),
    );
    cmd.create_response(&ctx.http, response).await?;
    Ok(())
}
