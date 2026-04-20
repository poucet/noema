//! /skills — list skills registered with the daemon.

use lumina_macros::command_group;
use serenity::all::{CommandInteraction, CreateInteractionResponse, CreateInteractionResponseMessage};
use simply_daemon_api::SkillsApi;

use super::LuminaContext;

#[command_group(description = "Skills registered with the daemon")]
mod skills {
    use super::*;

    #[sub_command(description = "List registered skills")]
    pub async fn list(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
    ) -> anyhow::Result<()> {
        let items = lx.daemon.skills().list_skills().await?;

        if items.is_empty() {
            return reply_ephemeral(lx, cmd, "No skills registered.").await;
        }

        let lines: Vec<String> = items.iter().map(|s| {
            let kind = match s.kind.as_str() {
                "embedded" => "in-process",
                "ws" => "WS client",
                other => other,
            };
            let status = if s.is_connected { "✅" } else { "❌" };
            let oauth = if s.oauth_provider_ids.is_empty() {
                String::new()
            } else {
                format!(" · needs: {}", s.oauth_provider_ids.join(", "))
            };
            format!(
                "{status} `{}` — {} ({}, {} tool{}){oauth}",
                s.id,
                s.display_name,
                kind,
                s.tool_count,
                if s.tool_count == 1 { "" } else { "s" },
            )
        }).collect();

        reply_ephemeral(lx, cmd, &lines.join("\n")).await
    }

    async fn reply_ephemeral(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        content: &str,
    ) -> anyhow::Result<()> {
        cmd.create_response(
            &lx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .ephemeral(true),
            ),
        )
        .await?;
        Ok(())
    }
}
