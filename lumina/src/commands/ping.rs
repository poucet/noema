//! /ping — health check command.

use async_trait::async_trait;
use serenity::all::CommandInteraction;
use serenity::builder::{CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::prelude::*;

use super::SlashCommand;
use crate::register_command;

#[derive(Default)]
pub struct Ping;

register_command!(Ping);

#[async_trait]
impl SlashCommand for Ping {
    fn name(&self) -> &'static str {
        "ping"
    }

    fn register(&self) -> CreateCommand {
        CreateCommand::new("ping").description("Check if Lumina is alive")
    }

    async fn run(&self, ctx: &Context, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let response = CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().content("Pong!"),
        );
        cmd.create_response(&ctx.http, response).await?;
        Ok(())
    }
}
