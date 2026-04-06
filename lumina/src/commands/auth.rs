//! /auth — link Discord account to the daemon via Google OAuth.

use lumina_macros::slash_command;
use serenity::all::CommandInteraction;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};

use super::LuminaContext;

#[slash_command(description = "Link your Discord account to the daemon via Google sign-in")]
async fn auth(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let settings = config::Settings::load();
    let port = settings.daemon_port.unwrap_or(config::DEFAULT_DAEMON_PORT);
    let discord_id = cmd.user.id.get().to_string();

    let auth_url = format!(
        "http://127.0.0.1:{port}/auth/login?discord_id={discord_id}"
    );

    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .content(format!(
                "Click below to link your Discord account:\n{}",
                auth_url,
            ))
            .ephemeral(true),
    );
    cmd.create_response(&lx.http, response).await?;
    Ok(())
}
