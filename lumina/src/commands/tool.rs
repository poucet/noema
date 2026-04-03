//! /tool — invoke and browse MCP tools from Discord.
//!
//! Subcommands:
//! - `/tool call <name>` — autocomplete + modal form → execute tool
//! - `/tool list` — paginated embed of all available tools

use async_trait::async_trait;
use serenity::all::{
    ActionRowComponent, AutocompleteChoice, CommandInteraction, CommandOptionType,
    CreateAutocompleteResponse, CreateInteractionResponse, CreateInteractionResponseMessage,
    InputTextStyle, ResolvedOption, ResolvedValue,
};
use serenity::builder::{
    CreateActionRow, CreateCommand, CreateCommandOption, CreateEmbed, CreateInputText, CreateModal,
};
use simply_daemon::api::{CallToolRequest, McpApi};
use std::time::Duration;

use super::LuminaContext;
use crate::register_command;

#[derive(Default)]
pub struct Tool;

register_command!(Tool);

const MODAL_PREFIX: &str = "tool_call:";

#[async_trait]
impl super::SlashCommand for Tool {
    fn name(&self) -> &'static str {
        "tool"
    }

    fn register(&self) -> CreateCommand {
        CreateCommand::new("tool")
            .description("MCP tool management")
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "call", "Call an MCP tool")
                    .add_sub_option(
                        CreateCommandOption::new(CommandOptionType::String, "name", "Tool name")
                            .required(true)
                            .set_autocomplete(true),
                    ),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "list", "List all available tools"),
            )
    }

    async fn run(&self, lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let opts = cmd.data.options();
        let sub = opts.first().ok_or_else(|| anyhow::anyhow!("missing subcommand"))?;

        match sub.name {
            "call" => {
                let tool_name = if let ResolvedValue::SubCommand(ref sub_opts) = sub.value {
                    sub_opts.iter().find_map(|o| match o {
                        ResolvedOption { name: "name", value: ResolvedValue::String(s), .. } => Some(*s),
                        _ => None,
                    })
                } else {
                    None
                }.ok_or_else(|| anyhow::anyhow!("missing tool name"))?;

                cmd_call(lx, cmd, tool_name).await
            }
            "list" => cmd_list(lx, cmd).await,
            other => Err(anyhow::anyhow!("unknown subcommand `{other}`")),
        }
    }

    async fn autocomplete(&self, lx: &LuminaContext, ac: &CommandInteraction) -> anyhow::Result<()> {
        let opts = ac.data.options();
        let sub = match opts.first() {
            Some(o) if o.name == "call" => o,
            _ => return Ok(()),
        };

        let partial = if let ResolvedValue::SubCommand(ref sub_opts) = sub.value {
            sub_opts.iter().find_map(|o| match o {
                ResolvedOption { name: "name", value: ResolvedValue::String(s), .. } => Some(s.to_lowercase()),
                _ => None,
            }).unwrap_or_default()
        } else {
            String::new()
        };

        let tools = lx.daemon.list_all_tools().await.unwrap_or_default();
        let choices: Vec<AutocompleteChoice> = tools
            .into_iter()
            .filter(|t| partial.is_empty() || t.name.to_lowercase().contains(&partial))
            .take(25)
            .map(|t| {
                let display = match &t.description {
                    Some(d) if d.len() <= 80 => format!("{} — {}", t.name, d),
                    Some(d) => format!("{} — {}...", t.name, &d[..77]),
                    None => t.name.clone(),
                };
                AutocompleteChoice::new(display, t.name)
            })
            .collect();

        ac.create_response(
            &lx.http,
            CreateInteractionResponse::Autocomplete(
                CreateAutocompleteResponse::new().set_choices(choices),
            ),
        ).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// /tool call
// ---------------------------------------------------------------------------

async fn cmd_call(lx: &LuminaContext, cmd: &CommandInteraction, tool_name: &str) -> anyhow::Result<()> {
    let tools = lx.daemon.list_all_tools().await?;
    let tool = tools
        .iter()
        .find(|t| t.name == tool_name)
        .ok_or_else(|| anyhow::anyhow!("tool not found: {tool_name}"))?;

    let params = extract_params(&tool.input_schema);

    if params.is_empty() {
        // No params — execute immediately
        let result = lx.daemon.call_tool_direct(CallToolRequest {
            name: tool_name.to_string(),
            arguments: serde_json::json!({}),
        }).await;
        send_result(lx, cmd, tool_name, result).await
    } else {
        // Build modal with input fields (Discord max: 5 components)
        let mut components = Vec::new();
        for param in params.iter().take(5) {
            let style = if matches!(param.name.as_str(), "content" | "description" | "prompt" | "question") {
                InputTextStyle::Paragraph
            } else {
                InputTextStyle::Short
            };
            let mut input = CreateInputText::new(style, &param.label, &param.name);
            if let Some(desc) = &param.description {
                input = input.placeholder(desc);
            }
            input = input.required(param.required);
            components.push(CreateActionRow::InputText(input));
        }

        let modal = CreateModal::new(
            format!("{MODAL_PREFIX}{tool_name}"),
            format!("Tool: {tool_name}"),
        ).components(components);

        cmd.create_response(&lx.http, CreateInteractionResponse::Modal(modal)).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// /tool list
// ---------------------------------------------------------------------------

async fn cmd_list(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
    let tools = lx.daemon.list_all_tools().await?;

    if tools.is_empty() {
        cmd.create_response(&lx.http, CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().content("No tools available.").ephemeral(true),
        )).await?;
        return Ok(());
    }

    // Group tools by server
    let mut by_server: std::collections::BTreeMap<String, Vec<&simply_daemon::api::McpToolInfo>> =
        std::collections::BTreeMap::new();
    for tool in &tools {
        by_server.entry(tool.server_id.clone()).or_default().push(tool);
    }

    // Build pages — each page lists tools from one or more servers, fitting within 4096 embed desc limit
    let mut pages: Vec<String> = Vec::new();
    let mut current_page = String::new();

    for (server_id, server_tools) in &by_server {
        let mut section = format!("### {server_id}\n");
        for tool in server_tools {
            let desc = tool.description.as_deref().unwrap_or("No description");
            let param_count = tool.input_schema
                .get("properties")
                .and_then(|p| p.as_object())
                .map(|p| p.len())
                .unwrap_or(0);
            section.push_str(&format!(
                "**`{}`** — {} ({} params)\n",
                tool.name, desc, param_count,
            ));
        }

        if !current_page.is_empty() && current_page.len() + section.len() > 3800 {
            pages.push(current_page);
            current_page = String::new();
        }
        current_page.push_str(&section);
        current_page.push('\n');
    }
    if !current_page.is_empty() {
        pages.push(current_page);
    }

    // Use paginator for multi-page, direct embed for single page
    if pages.len() == 1 {
        let embed = CreateEmbed::new()
            .title(format!("MCP Tools ({} total)", tools.len()))
            .description(&pages[0])
            .color(0x5865F2);
        cmd.create_response(&lx.http, CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().embed(embed),
        )).await?;
    } else {
        // Convert to paginator-compatible text pages with embed formatting
        let text_pages: Vec<String> = pages
            .iter()
            .enumerate()
            .map(|(i, content)| {
                format!("**MCP Tools** ({} total) — Page {}/{}\n\n{}", tools.len(), i + 1, pages.len(), content)
            })
            .collect();
        crate::paginator::send_paginated(lx, cmd, &text_pages, Duration::from_secs(120)).await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Modal handler
// ---------------------------------------------------------------------------

/// Handle a modal submission for a tool call.
pub async fn handle_modal(lx: &LuminaContext, modal: &serenity::model::application::ModalInteraction) -> anyhow::Result<()> {
    let tool_name = modal.data.custom_id
        .strip_prefix(MODAL_PREFIX)
        .ok_or_else(|| anyhow::anyhow!("not a tool modal"))?;

    let mut args = serde_json::Map::new();
    for row in &modal.data.components {
        if let Some(ActionRowComponent::InputText(input)) = row.components.first() {
            if let Some(ref value) = input.value {
                if !value.is_empty() {
                    args.insert(input.custom_id.clone(), parse_smart_value(value));
                }
            }
        }
    }

    // Defer since tool calls may take time
    modal.create_response(&lx.http, CreateInteractionResponse::Defer(
        CreateInteractionResponseMessage::new(),
    )).await?;

    let result = lx.daemon.call_tool_direct(CallToolRequest {
        name: tool_name.to_string(),
        arguments: serde_json::Value::Object(args),
    }).await;

    let (color, title, body) = format_result(tool_name, result);
    let embed = CreateEmbed::new().title(title).description(body).color(color);
    modal.edit_response(&lx.http, serenity::builder::EditInteractionResponse::new().embed(embed)).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct ParamInfo {
    name: String,
    label: String,
    description: Option<String>,
    required: bool,
}

fn extract_params(schema: &serde_json::Value) -> Vec<ParamInfo> {
    let properties = schema.get("properties").and_then(|p| p.as_object());
    let required_list: Vec<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let Some(props) = properties else { return vec![] };

    let mut params: Vec<ParamInfo> = props
        .iter()
        .map(|(name, prop)| {
            let description = prop.get("description").and_then(|d| d.as_str()).map(|s| s.to_string());
            let label = description.as_deref().unwrap_or(name).chars().take(45).collect::<String>();
            ParamInfo {
                name: name.clone(),
                label,
                description,
                required: required_list.contains(&name.as_str()),
            }
        })
        .collect();

    // Required params first
    params.sort_by_key(|p| !p.required);
    params
}

fn parse_smart_value(s: &str) -> serde_json::Value {
    if let Ok(n) = s.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    if let Ok(n) = s.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(n) {
            return serde_json::Value::Number(n);
        }
    }
    match s {
        "true" => return serde_json::Value::Bool(true),
        "false" => return serde_json::Value::Bool(false),
        _ => {}
    }
    if s.starts_with('[') || s.starts_with('{') {
        if let Ok(v) = serde_json::from_str(s) {
            return v;
        }
    }
    serde_json::Value::String(s.to_string())
}

fn format_result(tool_name: &str, result: anyhow::Result<simply_daemon::api::CallToolResult>) -> (u32, String, String) {
    match result {
        Ok(r) => {
            let text = r.content.iter()
                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n");
            let body = if text.len() > 4000 { format!("{}...", &text[..3997]) } else { text };
            (0x2ECC71, format!("Tool: {tool_name}"), body)
        }
        Err(e) => (0xE74C3C, format!("Tool: {tool_name} (error)"), format!("{e}")),
    }
}

async fn send_result(
    lx: &LuminaContext,
    cmd: &CommandInteraction,
    tool_name: &str,
    result: anyhow::Result<simply_daemon::api::CallToolResult>,
) -> anyhow::Result<()> {
    let (color, title, body) = format_result(tool_name, result);
    let embed = CreateEmbed::new().title(title).description(body).color(color);
    cmd.create_response(&lx.http, CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().embed(embed),
    )).await?;
    Ok(())
}
