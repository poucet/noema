//! /auth — link Discord account to the daemon via Google OAuth.

use lumina_macros::slash_command;
use serenity::all::CommandInteraction;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
use simply_daemon_api::Daemon;

use super::LuminaContext;

#[slash_command(description = "Link your Discord account to the daemon via Google sign-in")]
async fn auth(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let base_url = lx.daemon.core().public_url().await?;
    let external_id = format!("discord:{}", cmd.user.id.get());

    // Use the per-user OAuth flow at /auth/mcp/{provider} — the only auth path
    // exposed publicly (nginx allowlists /auth/mcp/*). `/auth/login` was never a
    // real route. The callback get-or-creates the user by Google email and links
    // this discord external_id to that user.
    let auth_url = format!(
        "{base_url}/auth/mcp/google?external_id={}",
        urlencoding::encode(&external_id),
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
