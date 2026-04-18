//! /google — Google Docs auth and import.
//!
//! Subcommands:
//! - `/google auth` — generate OAuth link for Google Docs
//! - `/google import <doc>` — import a Google Doc (autocomplete from Drive)
//! - `/google status` — check if Google is connected

use async_trait::async_trait;
use serenity::all::{
    AutocompleteChoice, CommandInteraction, CommandOptionType,
    CreateAutocompleteResponse, CreateInteractionResponse, CreateInteractionResponseMessage,
    ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateCommand, CreateCommandOption, CreateEmbed};
use simply_daemon_api::Daemon;

use super::LuminaContext;
use crate::register_command;

const GOOGLE_DOCS_SERVER_ID: &str = "google-docs";

#[derive(Default)]
pub struct Google;

register_command!(Google);

#[async_trait]
impl super::SlashCommand for Google {
    fn name(&self) -> &'static str { "google" }

    fn register(&self) -> CreateCommand {
        CreateCommand::new("google")
            .description("Google Docs integration")
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "auth", "Connect your Google account"),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "import", "Import a Google Doc")
                    .add_sub_option(
                        CreateCommandOption::new(CommandOptionType::String, "doc", "Google Doc to import (name or URL)")
                            .required(true)
                            .set_autocomplete(true),
                    ),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "status", "Check Google connection status"),
            )
    }

    async fn run(&self, lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let opts = cmd.data.options();
        let sub = opts.first().ok_or_else(|| anyhow::anyhow!("missing subcommand"))?;

        match sub.name {
            "auth" => cmd_auth(lx, cmd).await,
            "import" => {
                let doc_input = if let ResolvedValue::SubCommand(ref sub_opts) = sub.value {
                    sub_opts.iter().find_map(|o| match o {
                        ResolvedOption { name: "doc", value: ResolvedValue::String(s), .. } => Some(s.to_string()),
                        _ => None,
                    })
                } else { None }.ok_or_else(|| anyhow::anyhow!("missing doc argument"))?;
                cmd_import(lx, cmd, &doc_input).await
            }
            "status" => cmd_status(lx, cmd).await,
            other => Err(anyhow::anyhow!("unknown subcommand `{other}`")),
        }
    }

    async fn autocomplete(&self, lx: &LuminaContext, ac: &CommandInteraction) -> anyhow::Result<()> {
        let opts = ac.data.options();
        let sub = match opts.first() {
            Some(o) if o.name == "import" => o,
            _ => return Ok(()),
        };

        let partial = if let ResolvedValue::SubCommand(ref sub_opts) = sub.value {
            sub_opts.iter().find_map(|o| match o {
                ResolvedOption { name: "doc", value: ResolvedValue::String(s), .. } => Some(s.to_string()),
                _ => None,
            }).unwrap_or_default()
        } else { String::new() };

        // Use gdocs_list tool for autocomplete
        let choices = match call_tool(lx, ac.user.id.get(), "gdocs_list",
            serde_json::json!({ "query": if partial.is_empty() { None } else { Some(&partial) }, "limit": 25 }),
        ).await {
            Ok(text) => {
                let docs: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
                docs.iter().take(25).filter_map(|d| {
                    let id = d.get("id")?.as_str()?;
                    let name = d.get("name")?.as_str()?;
                    let display = if name.len() > 100 { format!("{}...", &name[..97]) } else { name.to_string() };
                    Some(AutocompleteChoice::new(display, id))
                }).collect()
            }
            Err(_) => vec![AutocompleteChoice::new("(connect Google first: /google auth)", "")],
        };

        ac.create_response(&lx.http, CreateInteractionResponse::Autocomplete(
            CreateAutocompleteResponse::new().set_choices(choices),
        )).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

async fn cmd_auth(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let discord_id = cmd.user.id.get();

    let external_id = format!("discord:{discord_id}");
    let ctx = lx.ctx_for(discord_id).await;
    let scope = lx.daemon.user().resolve_or_create_user(&ctx, external_id).await?;
    let user_id = scope.user_id.clone().unwrap_or_default();
    lx.register_user_scope(discord_id, scope).await;

    let base_url = lx.daemon.core().public_url().await?;
    let auth_url = format!("{base_url}/auth/mcp/{GOOGLE_DOCS_SERVER_ID}?user_id={user_id}");

    cmd.create_response(&lx.http, CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .content(format!("Click to connect Google:\n{auth_url}"))
            .ephemeral(true),
    )).await?;
    Ok(())
}

async fn cmd_import(lx: &LuminaContext, cmd: &CommandInteraction, doc_input: &str) -> anyhow::Result<()> {
    let discord_id = cmd.user.id.get();
    let doc_id = extract_doc_id(doc_input);

    lx.defer(cmd).await?;

    // Call gdocs_import skill tool — handles extraction, tab creation, image storage
    match call_tool(lx, discord_id, "gdocs_import", serde_json::json!({ "doc_id": doc_id })).await {
        Ok(text) => {
            let embed = CreateEmbed::new()
                .title("Google Doc Imported")
                .description(text)
                .color(0x14b8a6);
            cmd.edit_response(&lx.http, serenity::builder::EditInteractionResponse::new().embed(embed)).await?;
        }
        Err(e) => {
            let msg = format!("{e}");
            if msg.contains("authenticate") || msg.contains("token") {
                cmd.edit_response(&lx.http,
                    serenity::builder::EditInteractionResponse::new()
                        .content("Not connected to Google. Run `/google auth` first."),
                ).await?;
            } else {
                cmd.edit_response(&lx.http,
                    serenity::builder::EditInteractionResponse::new()
                        .content(format!("Import failed: {e}")),
                ).await?;
            }
        }
    }
    Ok(())
}

async fn cmd_status(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let servers = lx.daemon.mcp().list_mcp_servers().await?;
    let google = servers.iter().find(|s| s.id == GOOGLE_DOCS_SERVER_ID);

    let (status_text, color) = match google {
        Some(s) if s.is_connected => ("Connected", 0x14b8a6u32),
        Some(_) => ("Server configured but not connected", 0xf59e0bu32),
        None => ("Google Docs MCP server not configured", 0xef4444u32),
    };

    cmd.create_response(&lx.http, CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .embed(CreateEmbed::new().title("Google Docs Status").description(status_text).color(color))
            .ephemeral(true),
    )).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Call a daemon tool and return the text result.
async fn call_tool(lx: &LuminaContext, discord_user_id: u64, tool_name: &str, args: serde_json::Value) -> anyhow::Result<String> {
    let ctx = lx.ctx_for(discord_user_id).await;
    let request = simply_daemon_api::CallToolRequestParams::new(tool_name.to_string())
        .with_arguments(args.as_object().cloned().unwrap_or_default());
    let result = lx.daemon.mcp().call_tool_direct(&ctx, request).await?;

    if result.is_error.unwrap_or(false) {
        let text = result.content.iter().find_map(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.to_string()),
            _ => None,
        }).unwrap_or_default();
        anyhow::bail!(text);
    }

    Ok(result.content.iter().find_map(|c| match &c.raw {
        rmcp::model::RawContent::Text(t) => Some(t.text.to_string()),
        _ => None,
    }).unwrap_or_default())
}

fn extract_doc_id(input: &str) -> String {
    if input.contains("docs.google.com") {
        if let Some(start) = input.find("/d/") {
            let id_start = start + 3;
            let id_end = input[id_start..].find('/').map(|i| id_start + i).unwrap_or(input.len());
            return input[id_start..id_end].to_string();
        }
    }
    input.to_string()
}
