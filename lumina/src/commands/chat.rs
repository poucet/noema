//! /chat — echo command (placeholder for LLM chat in stage 2).

use lumina_macros::slash_command;
use serenity::all::CommandInteraction;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::prelude::*;

#[slash_command(description = "Chat with Lumina")]
async fn chat(
    ctx: &Context,
    cmd: &CommandInteraction,
    #[describe("Your message")] message: String,
) -> anyhow::Result<()> {
    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().content(format!("Echo: {message}")),
    );
    cmd.create_response(&ctx.http, response).await?;
    Ok(())
}
