//! /tool — invoke any MCP tool from Discord via a modal form.
//!
//! 1. `/tool` with autocomplete shows available tools
//! 2. Selecting a tool opens a modal with input fields per parameter
//! 3. Submit executes the tool and shows the result as an embed

use async_trait::async_trait;
use serenity::all::{
    ActionRowComponent, AutocompleteChoice, CommandInteraction, CommandOptionType,
    CreateAutocompleteResponse, CreateInteractionResponse, CreateInteractionResponseMessage,
    InputTextStyle, ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateActionRow, CreateCommand, CreateCommandOption, CreateEmbed, CreateInputText, CreateModal};
use simply_daemon::api::{CallToolRequest, McpApi};

use super::LuminaContext;
use crate::register_command;

#[derive(Default)]
pub struct Tool;

register_command!(Tool);

/// Custom ID prefix for tool modals — followed by the tool name.
const MODAL_PREFIX: &str = "tool_call:";

#[async_trait]
impl super::SlashCommand for Tool {
    fn name(&self) -> &'static str {
        "tool"
    }

    fn register(&self) -> CreateCommand {
        CreateCommand::new("tool")
            .description("Call an MCP tool")
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "name", "Tool name")
                    .required(true)
                    .set_autocomplete(true),
            )
    }

    async fn run(&self, lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let opts = cmd.data.options();
        let tool_name = opts
            .iter()
            .find_map(|o| match o {
                ResolvedOption { name: "name", value: ResolvedValue::String(s), .. } => Some(*s),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("missing tool name"))?;

        // Fetch tool schema to build modal fields
        let tools = lx.daemon.list_all_tools().await?;
        let tool = tools
            .iter()
            .find(|t| t.name == tool_name)
            .ok_or_else(|| anyhow::anyhow!("tool not found: {tool_name}"))?;

        // Extract properties from JSON schema
        let params = extract_params(&tool.input_schema);

        if params.is_empty() {
            // No params — execute immediately
            let result = lx
                .daemon
                .call_tool_direct(CallToolRequest {
                    name: tool_name.to_string(),
                    arguments: serde_json::json!({}),
                })
                .await;
            send_result_embed(lx, cmd, tool_name, result).await
        } else {
            // Build modal with input fields (Discord max: 5 components)
            let mut components = Vec::new();
            for (i, param) in params.iter().take(5).enumerate() {
                let style = if param.name == "content" || param.name == "description" || param.name == "prompt" {
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
            )
            .components(components);

            cmd.create_response(&lx.http, CreateInteractionResponse::Modal(modal)).await?;
            Ok(())
        }
    }

    async fn autocomplete(&self, lx: &LuminaContext, ac: &CommandInteraction) -> anyhow::Result<()> {
        let partial = ac
            .data
            .options()
            .iter()
            .find_map(|o| match o {
                ResolvedOption { name: "name", value: ResolvedValue::String(s), .. } => Some(s.to_lowercase()),
                _ => None,
            })
            .unwrap_or_default();

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
        )
        .await?;
        Ok(())
    }
}

/// Handle a modal submission for a tool call.
pub async fn handle_modal(lx: &LuminaContext, modal: &serenity::model::application::ModalInteraction) -> anyhow::Result<()> {
    let tool_name = modal
        .data
        .custom_id
        .strip_prefix(MODAL_PREFIX)
        .ok_or_else(|| anyhow::anyhow!("not a tool modal"))?;

    // Extract field values from modal submission
    let mut args = serde_json::Map::new();
    for row in &modal.data.components {
        if let Some(ActionRowComponent::InputText(input)) = row.components.first() {
            if let Some(ref value) = input.value {
                if !value.is_empty() {
                    // Try to parse as number/bool/json, fallback to string
                    let json_value = parse_smart_value(value);
                    args.insert(input.custom_id.clone(), json_value);
                }
            }
        }
    }

    // Defer response since tool calls may take time
    modal
        .create_response(&lx.http, CreateInteractionResponse::Defer(
            CreateInteractionResponseMessage::new().ephemeral(false),
        ))
        .await?;

    let result = lx
        .daemon
        .call_tool_direct(CallToolRequest {
            name: tool_name.to_string(),
            arguments: serde_json::Value::Object(args),
        })
        .await;

    let (color, title, body) = match result {
        Ok(r) => {
            let text = r.content
                .iter()
                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n");
            let body = if text.len() > 4000 { format!("{}...", &text[..3997]) } else { text };
            (0x2ECC71, format!("Tool: {tool_name}"), body)
        }
        Err(e) => (0xE74C3C, format!("Tool: {tool_name} (error)"), format!("{e}")),
    };

    let embed = CreateEmbed::new().title(title).description(body).color(color);
    modal
        .edit_response(&lx.http, serenity::builder::EditInteractionResponse::new().embed(embed))
        .await?;
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

/// Parse a string value smartly — integers, booleans, JSON arrays/objects, or plain string.
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
    // Try JSON array/object
    if (s.starts_with('[') || s.starts_with('{')) {
        if let Ok(v) = serde_json::from_str(s) {
            return v;
        }
    }
    serde_json::Value::String(s.to_string())
}

/// Send a tool result as an embed (for no-param tools that execute immediately).
async fn send_result_embed(
    lx: &LuminaContext,
    cmd: &CommandInteraction,
    tool_name: &str,
    result: anyhow::Result<simply_daemon::api::CallToolResult>,
) -> anyhow::Result<()> {
    let (color, title, body) = match result {
        Ok(r) => {
            let text = r.content
                .iter()
                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n");
            let body = if text.len() > 4000 { format!("{}...", &text[..3997]) } else { text };
            (0x2ECC71, format!("Tool: {tool_name}"), body)
        }
        Err(e) => (0xE74C3C, format!("Tool: {tool_name} (error)"), format!("{e}")),
    };

    let embed = CreateEmbed::new().title(title).description(body).color(color);
    cmd.create_response(
        &lx.http,
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().embed(embed),
        ),
    )
    .await?;
    Ok(())
}
