//! Slash command registration and dispatch.

mod chat;
mod ping;

use serenity::all::CommandInteraction;
use serenity::builder::CreateCommand;
use serenity::prelude::*;

/// Return all slash command definitions for guild registration.
pub fn register() -> Vec<CreateCommand> {
    vec![ping::register(), chat::register()]
}

/// Dispatch an incoming slash command to the right handler.
pub async fn handle(ctx: &Context, cmd: &CommandInteraction) {
    let result = match cmd.data.name.as_str() {
        "ping" => ping::run(ctx, cmd).await,
        "chat" => chat::run(ctx, cmd).await,
        _ => {
            tracing::warn!(command = %cmd.data.name, "unknown command");
            return;
        }
    };

    if let Err(e) = result {
        tracing::error!(command = %cmd.data.name, error = %e, "command failed");
    }
}
