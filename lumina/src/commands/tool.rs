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
    CreateActionRow, CreateCommand, CreateCommandOption, CreateEmbed, CreateInputText,
    CreateMessage, CreateModal,
};
use simply_daemon_api::Daemon; // ToolService is accessed via daemon.tools()
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

        let partial = ac.data.autocomplete()
            .map(|opt| opt.value.to_lowercase())
            .unwrap_or_default();

        let tools = lx.daemon.tools().get_definitions().await;
        let choices: Vec<AutocompleteChoice> = tools
            .into_iter()
            .filter(|t| partial.is_empty() || t.name.to_lowercase().contains(&partial))
            .take(25)
            .map(|t| {
                let display = match t.description.as_deref() {
                    Some(d) => {
                        let full = format!("{} — {d}", t.name);
                        if full.len() <= 100 { full }
                        else {
                            let max_desc = 97_usize.saturating_sub(t.name.len() + 4);
                            if max_desc > 3 { format!("{} — {}...", t.name, &d[..max_desc]) }
                            else { t.name.chars().take(100).collect() }
                        }
                    }
                    None => t.name.chars().take(100).collect(),
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
    let tools = lx.daemon.tools().get_definitions().await;
    let tool = tools
        .iter()
        .find(|t| t.name == tool_name)
        .ok_or_else(|| anyhow::anyhow!("tool not found: {tool_name}"))?;

    let schema = serde_json::to_value(&tool.input_schema).unwrap_or_default();
    let params = extract_params(&schema);

    if params.is_empty() {
        // No params — execute immediately
        let result = lx.daemon.tools()
            .call_tool(tool_name, serde_json::Value::Object(Default::default()))
            .await;
        send_tool_result_from_service(lx, cmd, tool_name, result).await
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
            // Pre-populate contextual Discord IDs
            match param.name.as_str() {
                "guild_id" => if let Some(gid) = cmd.guild_id {
                    input = input.value(gid.get().to_string());
                },
                "channel_id" => {
                    input = input.value(cmd.channel_id.get().to_string());
                },
                _ => {}
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
    let tools = lx.daemon.tools().get_definitions().await;

    if tools.is_empty() {
        cmd.create_response(&lx.http, CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().content("No tools available.").ephemeral(true),
        )).await?;
        return Ok(());
    }

    let mut text = String::new();
    for tool in &tools {
        let desc = tool.description.as_deref().unwrap_or("No description");
        let param_count = tool.input_schema.get("properties")
            .and_then(|p| p.as_object())
            .map(|p| p.len())
            .unwrap_or(0);
        text.push_str(&format!("**`{}`** — {desc} ({param_count} params)\n", tool.name));
    }

    let pages = crate::paginator::paginate_text(&text, 1800);

    if pages.len() <= 1 {
        let embed = CreateEmbed::new()
            .title(format!("Tools ({} total)", tools.len()))
            .description(pages.first().map(|s| s.as_str()).unwrap_or(""))
            .color(0x5865F2);
        cmd.create_response(&lx.http, CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().embed(embed),
        )).await?;
    } else {
        let text_pages: Vec<String> = pages.iter().enumerate().map(|(i, content)| {
            format!("**Tools** ({} total) — Page {}/{}\n\n{}", tools.len(), i + 1, pages.len(), content)
        }).collect();
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

    let result = lx.daemon.tools()
        .call_tool(tool_name, serde_json::Value::Object(args))
        .await;

    send_tool_result_from_service(lx, modal.channel_id, tool_name, result).await?;
    modal.delete_response(&lx.http).await.ok();
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

/// Format a CallToolResult into Discord messages (embed + optional attachments).
async fn send_tool_result(
    lx: &LuminaContext,
    channel_id: serenity::model::id::ChannelId,
    tool_name: &str,
    result: anyhow::Result<simply_daemon_api::CallToolResult>,
) -> anyhow::Result<()> {
    match result {
        Ok(r) => {
            let is_error = r.is_error.unwrap_or(false);
            let color = if is_error { 0xE74C3C } else { 0x2ECC71 };
            let mut text_parts = Vec::new();

            for content in &r.content {
                match &content.raw {
                    rmcp::model::RawContent::Text(t) => {
                        text_parts.push(t.text.to_string());
                    }
                    rmcp::model::RawContent::Image(img) => {
                        if let Ok(bytes) = base64_decode(&img.data) {
                            let ext = img.mime_type.split('/').last().unwrap_or("png");
                            let attachment = serenity::builder::CreateAttachment::bytes(bytes, format!("result.{ext}"));
                            channel_id.send_message(&lx.http, CreateMessage::new().add_file(attachment)).await?;
                        }
                    }
                    rmcp::model::RawContent::Audio(audio) => {
                        if let Ok(bytes) = base64_decode(&audio.data) {
                            let ext = audio.mime_type.split('/').last().unwrap_or("mp3");
                            let attachment = serenity::builder::CreateAttachment::bytes(bytes, format!("result.{ext}"));
                            channel_id.send_message(&lx.http, CreateMessage::new().add_file(attachment)).await?;
                        }
                    }
                    rmcp::model::RawContent::Resource(res) => {
                        match &res.resource {
                            rmcp::model::ResourceContents::TextResourceContents { text, .. } => {
                                text_parts.push(text.to_string());
                            }
                            rmcp::model::ResourceContents::BlobResourceContents { blob, mime_type, .. } => {
                                if let Ok(bytes) = base64_decode(blob) {
                                    let ext = mime_type.as_deref().and_then(|m| m.split('/').last()).unwrap_or("bin");
                                    let attachment = serenity::builder::CreateAttachment::bytes(bytes, format!("result.{ext}"));
                                    channel_id.send_message(&lx.http, CreateMessage::new().add_file(attachment)).await?;
                                }
                            }
                        }
                    }
                    rmcp::model::RawContent::ResourceLink(link) => {
                        text_parts.push(format!("Resource: {}", link.uri));
                    }
                }
            }

            if let Some(structured) = &r.structured_content {
                text_parts.push(serde_json::to_string_pretty(structured).unwrap_or_default());
            }

            let body = text_parts.join("\n");
            let body = if body.len() > 4000 { format!("{}...", &body[..3997]) } else { body };
            let embed = CreateEmbed::new()
                .title(format!("Tool: {tool_name}"))
                .description(body)
                .color(color);
            channel_id.send_message(&lx.http, CreateMessage::new().embed(embed)).await?;
        }
        Err(e) => {
            let embed = CreateEmbed::new()
                .title(format!("Tool: {tool_name} (error)"))
                .description(format!("{e}"))
                .color(0xE74C3C);
            channel_id.send_message(&lx.http, CreateMessage::new().embed(embed)).await?;
        }
    }
    Ok(())
}

/// Format a JSON value into a human-readable Discord-friendly string.
pub fn format_tool_output(text: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return text.to_string();
    };

    match &value {
        serde_json::Value::Array(items) => format_array(items),
        serde_json::Value::Object(obj) => format_object(obj, 0),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| text.to_string()),
    }
}

fn format_array(items: &[serde_json::Value]) -> String {
    let mut lines = Vec::new();
    lines.push(format!("**{} items:**", items.len()));

    for (i, item) in items.iter().enumerate() {
        if let serde_json::Value::Object(obj) = item {
            // Try to find a good label: name, id, title
            let label_key = ["name", "title", "id"].iter()
                .find(|&&k| obj.contains_key(k))
                .copied();

            let label = label_key.and_then(|k| obj.get(k)).and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.as_str()),
                serde_json::Value::Object(inner) => inner.values().find_map(|v| v.as_str()),
                _ => None,
            });

            if let Some(label) = label {
                // Don't wrap in bold if label is a URL or has markdown
                if label.starts_with("http") || label.contains('[') || label.contains('*') {
                    lines.push(format!("{}. {}", i + 1, label));
                } else {
                    lines.push(format!("{}. **{}**", i + 1, label));
                }
            } else {
                lines.push(format!("{}.", i + 1));
            }

            // Show key fields inline, skip the one used as label
            for (key, val) in obj {
                if label_key == Some(key.as_str()) { continue; }
                match val {
                    serde_json::Value::String(s) => {
                        lines.push(format!("   {key}: {s}"));
                    }
                    serde_json::Value::Bool(b) => {
                        lines.push(format!("   {key}: {b}"));
                    }
                    serde_json::Value::Number(n) => {
                        lines.push(format!("   {key}: {n}"));
                    }
                    serde_json::Value::Array(arr) => {
                        let summary: Vec<String> = arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                        if !summary.is_empty() {
                            lines.push(format!("   {key}: {}", summary.join(", ")));
                        }
                    }
                    serde_json::Value::Object(inner) => {
                        // Flatten simple inner objects (like "id": {"provider": "x", "model": "y"})
                        let flat: Vec<String> = inner.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| format!("{k}: {s}")))
                            .collect();
                        if !flat.is_empty() {
                            lines.push(format!("   {key}: {}", flat.join(", ")));
                        }
                    }
                    serde_json::Value::Null => {}
                }
            }
        } else if let Some(s) = item.as_str() {
            lines.push(format!("{}. {s}", i + 1));
        } else {
            lines.push(format!("{}. {}", i + 1, item));
        }
    }

    lines.join("\n")
}

fn format_object(obj: &serde_json::Map<String, serde_json::Value>, indent: usize) -> String {
    let prefix = "  ".repeat(indent);
    let mut lines = Vec::new();
    for (key, val) in obj {
        match val {
            serde_json::Value::String(s) => lines.push(format!("{prefix}**{key}:** {s}")),
            serde_json::Value::Bool(b) => lines.push(format!("{prefix}**{key}:** {b}")),
            serde_json::Value::Number(n) => lines.push(format!("{prefix}**{key}:** {n}")),
            serde_json::Value::Null => lines.push(format!("{prefix}**{key}:** —")),
            serde_json::Value::Array(arr) if arr.is_empty() => lines.push(format!("{prefix}**{key}:** (empty)")),
            serde_json::Value::Array(arr) => {
                let simple: Vec<String> = arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if simple.len() == arr.len() {
                    lines.push(format!("{prefix}**{key}:** {}", simple.join(", ")));
                } else {
                    // Expand arrays of objects inline
                    lines.push(format!("{prefix}**{key}:**"));
                    lines.push(format_array(arr));
                }
            }
            serde_json::Value::Object(inner) => {
                lines.push(format!("{prefix}**{key}:**"));
                lines.push(format_object(inner, indent + 1));
            }
        }
    }
    lines.join("\n")
}

/// Send a ToolService result (Vec<ToolResultContent>) as a Discord response.
async fn send_tool_result_from_service(
    lx: &LuminaContext,
    target: impl Into<SendTarget<'_>>,
    tool_name: &str,
    result: anyhow::Result<Vec<simply_daemon_api::ToolResultContent>>,
) -> anyhow::Result<()> {
    let target = target.into();

    // Defer the response first (tool calls may take time)
    let channel_id = match &target {
        SendTarget::Channel(id) => *id,
        SendTarget::Command(cmd) => {
            cmd.create_response(&lx.http, CreateInteractionResponse::Defer(
                CreateInteractionResponseMessage::new(),
            )).await?;
            cmd.channel_id
        }
    };

    match result {
        Ok(content) => {
            use simply_daemon_api::ToolResultContent;
            use base64::Engine;

            let mut text_parts = Vec::new();
            let mut attachments: Vec<serenity::builder::CreateAttachment> = Vec::new();

            for c in &content {
                match c {
                    ToolResultContent::Text { text } => text_parts.push(text.as_str()),
                    ToolResultContent::Image { data, mime_type } => {
                        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                            let ext = mime_type.split('/').last().unwrap_or("png");
                            attachments.push(serenity::builder::CreateAttachment::bytes(
                                bytes, format!("asset.{ext}"),
                            ));
                        }
                    }
                    ToolResultContent::Audio { data, mime_type } => {
                        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                            let ext = mime_type.split('/').last().unwrap_or("wav");
                            attachments.push(serenity::builder::CreateAttachment::bytes(
                                bytes, format!("audio.{ext}"),
                            ));
                        }
                    }
                }
            }

            if !attachments.is_empty() {
                let mut msg = CreateMessage::new();
                for a in attachments {
                    msg = msg.add_file(a);
                }
                if !text_parts.is_empty() {
                    msg = msg.content(text_parts.join("\n"));
                }
                channel_id.send_message(&lx.http, msg).await?;
            } else {
                let raw_text = text_parts.join("\n");
                let formatted = format_tool_output(&raw_text);
                let pages = crate::paginator::paginate_text(&formatted, 1800);

                crate::paginator::send_paginated_embeds_to_channel(
                    lx, channel_id, tool_name, &pages, Duration::from_secs(120),
                ).await?;
            }
        }
        Err(e) => {
            let embed = CreateEmbed::new()
                .title(format!("Tool: {tool_name} (error)"))
                .description(format!("{e}"))
                .color(0xE74C3C);
            channel_id.send_message(&lx.http, CreateMessage::new().embed(embed)).await?;
        }
    }

    // Clean up the deferred "thinking" message
    if let SendTarget::Command(cmd) = target {
        cmd.delete_response(&lx.http).await.ok();
    }
    Ok(())
}

enum SendTarget<'a> {
    Channel(serenity::model::id::ChannelId),
    Command(&'a CommandInteraction),
}

impl<'a> From<&'a CommandInteraction> for SendTarget<'a> {
    fn from(cmd: &'a CommandInteraction) -> Self { Self::Command(cmd) }
}

impl From<serenity::model::id::ChannelId> for SendTarget<'_> {
    fn from(id: serenity::model::id::ChannelId) -> Self { Self::Channel(id) }
}

fn base64_decode(data: &str) -> anyhow::Result<Vec<u8>> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.decode(data)?)
}

async fn send_result(
    lx: &LuminaContext,
    cmd: &CommandInteraction,
    tool_name: &str,
    result: anyhow::Result<simply_daemon_api::CallToolResult>,
) -> anyhow::Result<()> {
    // Acknowledge first, then send result to the channel
    cmd.create_response(&lx.http, CreateInteractionResponse::Defer(
        CreateInteractionResponseMessage::new(),
    )).await?;
    send_tool_result(lx, cmd.channel_id, tool_name, result).await?;
    // Delete the deferred "thinking" message
    cmd.delete_response(&lx.http).await.ok();
    Ok(())
}
