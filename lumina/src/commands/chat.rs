//! /chat — echo command (placeholder for LLM chat in stage 2).

use serenity::all::{CommandInteraction, CommandOptionType, ResolvedOption, ResolvedValue};
use serenity::builder::{
    CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use serenity::prelude::*;

pub fn register() -> CreateCommand {
    CreateCommand::new("chat")
        .description("Chat with Lumina")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "message", "Your message")
                .required(true),
        )
}

pub async fn run(ctx: &Context, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let message = cmd
        .data
        .options()
        .iter()
        .find_map(|opt| match opt {
            ResolvedOption {
                name: "message",
                value: ResolvedValue::String(s),
                ..
            } => Some(*s),
            _ => None,
        })
        .unwrap_or("(empty)");

    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().content(format!("Echo: {message}")),
    );
    cmd.create_response(&ctx.http, response).await?;
    Ok(())
}
