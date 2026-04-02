//! /chat — echo command (placeholder for LLM chat in stage 2).

use async_trait::async_trait;
use serenity::all::{CommandInteraction, CommandOptionType, ResolvedOption, ResolvedValue};
use serenity::builder::{
    CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use serenity::prelude::*;

use super::SlashCommand;
use crate::register_command;

#[derive(Default)]
pub struct Chat;

register_command!(Chat);

#[async_trait]
impl SlashCommand for Chat {
    fn name(&self) -> &'static str {
        "chat"
    }

    fn register(&self) -> CreateCommand {
        CreateCommand::new("chat")
            .description("Chat with Lumina")
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "message", "Your message")
                    .required(true),
            )
    }

    async fn run(&self, ctx: &Context, cmd: &CommandInteraction) -> anyhow::Result<()> {
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
}
