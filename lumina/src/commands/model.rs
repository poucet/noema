//! /model — model listing and management.

use lumina_macros::command_group;
use serenity::all::{CommandInteraction, CreateInteractionResponse, CreateInteractionResponseMessage};
use simply_daemon::api::ModelApi;

use super::LuminaContext;

#[command_group(description = "LLM model management")]
mod model {
    use super::*;

    #[sub_command(description = "List available models")]
    pub async fn list(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("Filter by provider name")] provider: Option<String>,
    ) -> anyhow::Result<()> {
        let models = lx.daemon.list_models().await?;

        let filtered: Vec<_> = match &provider {
            Some(p) => {
                let p_lower = p.to_lowercase();
                models.iter().filter(|m| m.id.provider.to_lowercase().contains(&p_lower)).collect()
            }
            None => models.iter().collect(),
        };

        if filtered.is_empty() {
            let msg = match &provider {
                Some(p) => format!("No models found for provider `{p}`"),
                None => "No models available".to_string(),
            };
            return reply_ephemeral(lx, cmd, &msg).await;
        }

        // Group by provider
        let mut by_provider: std::collections::BTreeMap<&str, Vec<String>> =
            std::collections::BTreeMap::new();
        for m in &filtered {
            by_provider
                .entry(&m.id.provider)
                .or_default()
                .push(format!(
                    "`{}` — {}",
                    m.id,
                    m.definition.display_name.as_deref().unwrap_or(&m.id.model)
                ));
        }

        let mut lines = Vec::new();
        for (provider, models) in &by_provider {
            lines.push(format!("**{provider}** ({} models)", models.len()));
            for model in models {
                lines.push(format!("  {model}"));
            }
        }

        let text = lines.join("\n");
        reply_ephemeral(lx, cmd, &truncate(&text, 2000)).await
    }

    #[sub_command(description = "Show current model for this channel")]
    pub async fn current(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
    ) -> anyhow::Result<()> {
        let channel_id = cmd.channel_id;
        let channel_model = lx.state.channel_models.read().await.get(&channel_id).cloned();
        let config_default = lx.config.discord.model_id.as_deref().filter(|s| !s.is_empty());

        let (source, model) = match (&channel_model, config_default) {
            (Some(m), _) => ("channel", m.as_str()),
            (None, Some(m)) => ("config default", m),
            (None, None) => ("daemon default", "(not set)"),
        };

        reply_ephemeral(lx, cmd, &format!("Current model: `{model}` (source: {source})")).await
    }

    #[sub_command(description = "List available providers")]
    pub async fn providers(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
    ) -> anyhow::Result<()> {
        let providers = lx.daemon.list_providers().await;
        if providers.is_empty() {
            return reply_ephemeral(lx, cmd, "No providers configured").await;
        }

        let lines: Vec<String> = providers
            .iter()
            .map(|p| {
                let env = p.api_key_env.as_deref().unwrap_or("n/a");
                format!("`{}` — env: `{}`", p.name, env)
            })
            .collect();

        reply_ephemeral(lx, cmd, &lines.join("\n")).await
    }

    fn truncate(s: &str, max: usize) -> String {
        if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max - 3]) }
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
