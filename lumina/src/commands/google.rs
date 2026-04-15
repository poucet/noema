//! /google — Google Docs auth and import via MCP.
//!
//! Subcommands:
//! - `/google auth` — generate OAuth link for Google Docs MCP server
//! - `/google import <doc>` — import a Google Doc (autocomplete from user's Drive)
//! - `/google status` — check if Google is connected

use async_trait::async_trait;
use serenity::all::{
    AutocompleteChoice, CommandInteraction, CommandOptionType,
    CreateAutocompleteResponse, CreateInteractionResponse, CreateInteractionResponseMessage,
    ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateCommand, CreateCommandOption, CreateEmbed};
use simply_daemon::api::Daemon;

use super::LuminaContext;
use crate::register_command;

/// The MCP server ID for Google Docs (must match mcp.toml config).
const GOOGLE_DOCS_SERVER_ID: &str = "google-docs";

#[derive(Default)]
pub struct Google;

register_command!(Google);

#[async_trait]
impl super::SlashCommand for Google {
    fn name(&self) -> &'static str {
        "google"
    }

    fn register(&self) -> CreateCommand {
        CreateCommand::new("google")
            .description("Google Docs integration")
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "auth", "Connect your Google account"),
            )
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "import",
                    "Import a Google Doc into the knowledge base",
                )
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
                } else {
                    None
                }.ok_or_else(|| anyhow::anyhow!("missing doc argument"))?;

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
        } else {
            String::new()
        };

        // Call the gdocs_list MCP tool to get user's docs for autocomplete
        let choices = match list_user_docs(lx, if partial.is_empty() { None } else { Some(&partial) }).await {
            Ok(docs) => docs.into_iter().take(25).map(|(id, name)| {
                AutocompleteChoice::new(
                    if name.len() > 100 { format!("{}...", &name[..97]) } else { name },
                    id,
                )
            }).collect(),
            Err(_) => vec![AutocompleteChoice::new("(connect Google first: /google auth)", "")],
        };

        ac.create_response(&lx.http, CreateInteractionResponse::Autocomplete(
            CreateAutocompleteResponse::new().set_choices(choices),
        )).await?;

        Ok(())
    }
}

async fn cmd_auth(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let discord_id = cmd.user.id.get();

    // Create/resolve the daemon user for this Discord user
    let external_id = format!("discord:{discord_id}");
    let ctx = lx.ctx_for(discord_id).await;
    let scope = lx.daemon.user().resolve_or_create_user(&ctx, external_id).await?;
    let user_id = scope.user_id.clone().unwrap_or_default();

    // Cache the scope so all subsequent commands use it
    lx.register_user_scope(discord_id, scope).await;

    let base_url = lx.daemon.core().public_url().await?;

    let auth_url = format!("{base_url}/auth/mcp/{GOOGLE_DOCS_SERVER_ID}?user_id={user_id}");

    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .content(format!(
                "Click below to connect your Google account:\n{}",
                auth_url,
            ))
            .ephemeral(true),
    );
    cmd.create_response(&lx.http, response).await?;
    Ok(())
}

async fn cmd_import(lx: &LuminaContext, cmd: &CommandInteraction, doc_input: &str) -> anyhow::Result<()> {
    // Extract doc_id — could be a raw ID from autocomplete, or a URL
    let doc_id = extract_doc_id(doc_input);
    let ctx = lx.ctx_for(cmd.user.id.get()).await;

    lx.defer(cmd).await?;

    // Call the gdocs_extract MCP tool
    let args = serde_json::json!({ "doc_id": doc_id });
    let result = lx.daemon.tools().call_tool("gdocs_extract", args).await;

    match result {
        Ok(content) => {
            // Parse the extracted document from tool result
            let text: String = content.iter().find_map(|c| match c {
                simply_daemon::types::ToolResultContent::Text { text } => Some(text.to_string()),
                _ => None,
            }).unwrap_or_default();

            // Try to parse as ExtractedDocument JSON and create via DocumentApi
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(extracted) => {
                    let title = extracted.get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Imported Google Doc");

                    // Create document via DocumentApi
                    let doc_request = simply_daemon::api::CreateDocumentRequest {
                        title: title.to_string(),
                        document_type: Some("knowledge".to_string()),
                        content: Some(text.clone()),
                    };

                    match lx.daemon.document().create_document(&ctx, doc_request).await {
                        Ok(doc_info) => {
                            let embed = CreateEmbed::new()
                                .title(format!("Imported: {}", doc_info.title))
                                .description(format!(
                                    "Type: {}\nTabs: {}\nNow searchable via RAG",
                                    doc_info.document_type, doc_info.tab_count,
                                ))
                                .color(0x14b8a6);

                            cmd.edit_response(&lx.http, serenity::builder::EditInteractionResponse::new().embed(embed)).await?;
                        }
                        Err(e) => {
                            cmd.edit_response(&lx.http,
                                serenity::builder::EditInteractionResponse::new()
                                    .content(format!("Failed to store document: {e}"))
                            ).await?;
                        }
                    }
                }
                Err(_) => {
                    // Raw text result — store as-is
                    let doc_request = simply_daemon::api::CreateDocumentRequest {
                        title: format!("Google Doc {}", &doc_id[..8.min(doc_id.len())]),
                        document_type: Some("knowledge".to_string()),
                        content: Some(text.to_string()),
                    };
                    lx.daemon.document().create_document(&ctx, doc_request).await?;

                    cmd.edit_response(&lx.http,
                        serenity::builder::EditInteractionResponse::new()
                            .content("Document imported and indexed for RAG.")
                    ).await?;
                }
            }
        }
        Err(e) => {
            let msg = format!("{e}");
            if msg.contains("auth_required") || msg.contains("not found") {
                cmd.edit_response(&lx.http,
                    serenity::builder::EditInteractionResponse::new()
                        .content("Not connected to Google. Run `/google auth` first.")
                ).await?;
            } else {
                cmd.edit_response(&lx.http,
                    serenity::builder::EditInteractionResponse::new()
                        .content(format!("Import failed: {e}"))
                ).await?;
            }
        }
    }

    Ok(())
}

async fn cmd_status(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    // Check if the google-docs MCP server is connected and the user has tools
    let servers = lx.daemon.mcp().list_mcp_servers().await?;
    let google = servers.iter().find(|s| s.id == GOOGLE_DOCS_SERVER_ID);

    let (status_text, color) = match google {
        Some(s) if s.is_connected => ("Connected", 0x14b8a6u32),
        Some(_) => ("Server configured but not connected", 0xf59e0bu32),
        None => ("Google Docs MCP server not configured", 0xef4444u32),
    };

    let embed = CreateEmbed::new()
        .title("Google Docs Status")
        .description(status_text)
        .color(color);

    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .embed(embed)
            .ephemeral(true),
    );
    cmd.create_response(&lx.http, response).await?;
    Ok(())
}

/// List user's Google Docs via the gdocs_list MCP tool.
async fn list_user_docs(lx: &LuminaContext, query: Option<&str>) -> anyhow::Result<Vec<(String, String)>> {
    let mut args = serde_json::Map::new();
    if let Some(q) = query {
        args.insert("query".to_string(), serde_json::Value::String(q.to_string()));
    }
    args.insert("limit".to_string(), serde_json::Value::Number(25.into()));

    let result = lx.daemon.tools().call_tool(
        "gdocs_list",
        serde_json::Value::Object(args),
    ).await?;

    // Parse the text result as JSON array of {id, name}
    let text: String = result.iter().find_map(|c| match c {
        simply_daemon::types::ToolResultContent::Text { text } => Some(text.to_string()),
        _ => None,
    }).unwrap_or_default();

    let docs: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
    Ok(docs.iter().filter_map(|d| {
        let id = d.get("id").and_then(|v| v.as_str())?;
        let name = d.get("name").and_then(|v| v.as_str())?;
        Some((id.to_string(), name.to_string()))
    }).collect())
}

/// Extract a Google Doc ID from a URL or return the input as-is.
fn extract_doc_id(input: &str) -> String {
    // Handle URLs like https://docs.google.com/document/d/DOC_ID/edit
    if input.contains("docs.google.com") {
        if let Some(start) = input.find("/d/") {
            let id_start = start + 3;
            let id_end = input[id_start..].find('/').map(|i| id_start + i).unwrap_or(input.len());
            return input[id_start..id_end].to_string();
        }
    }
    input.to_string()
}
