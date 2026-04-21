//! Shared tool-result rendering for Discord — formatting, JSON-aware
//! pagination, and channel output. Used by `/tool call` (via `tool.rs`)
//! and the voice session receiver so MCP tool calls look the same in
//! both surfaces.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use serde_json::Value;
use serenity::all::ShardMessenger;
use serenity::builder::{CreateAttachment, CreateEmbed, CreateMessage};
use serenity::http::Http;
use serenity::model::id::ChannelId;
use simply_daemon_api::ToolResultContent;

use crate::paginator::{paginate_text, send_paginated_embeds_to_channel};

const PAGE_MAX_CHARS: usize = 1800;
const PAGINATION_TIMEOUT: Duration = Duration::from_secs(120);

/// Format a JSON value into a human-readable Discord-friendly string.
pub fn format_tool_output(text: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return text.to_string();
    };
    match &value {
        Value::Array(items) => format_array(items),
        Value::Object(obj) => format_object(obj, 0),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| text.to_string()),
    }
}

fn format_array(items: &[Value]) -> String {
    let mut lines = Vec::with_capacity(items.len() + 1);
    lines.push(format!("**{} items:**", items.len()));
    for (i, item) in items.iter().enumerate() {
        lines.push(format_array_item(item, i));
    }
    lines.join("\n")
}

/// Render one element of a JSON array as a single block (may contain
/// newlines internally, but pagination treats it as indivisible).
fn format_array_item(item: &Value, i: usize) -> String {
    let idx = i + 1;
    match item {
        Value::Object(obj) => {
            let mut lines = Vec::new();
            let label_key = ["name", "title", "id"].iter()
                .find(|&&k| obj.contains_key(k))
                .copied();
            let label = label_key.and_then(|k| obj.get(k)).and_then(|v| match v {
                Value::String(s) => Some(s.as_str()),
                Value::Object(inner) => inner.values().find_map(|v| v.as_str()),
                _ => None,
            });
            match label {
                Some(l) if l.starts_with("http") || l.contains('[') || l.contains('*') => {
                    lines.push(format!("{idx}. {l}"));
                }
                Some(l) => lines.push(format!("{idx}. **{l}**")),
                None => lines.push(format!("{idx}.")),
            }
            for (key, val) in obj {
                if label_key == Some(key.as_str()) { continue; }
                if let Some(rendered) = format_inline_field(key, val) {
                    lines.push(format!("   {rendered}"));
                }
            }
            lines.join("\n")
        }
        Value::String(s) => format!("{idx}. {s}"),
        other => format!("{idx}. {other}"),
    }
}

/// Render a single `key: value` pair as it appears inline under an array
/// item. Returns `None` for values that render to nothing (empty arrays,
/// nulls) so the caller can skip them.
fn format_inline_field(key: &str, val: &Value) -> Option<String> {
    match val {
        Value::String(s) => Some(format!("{key}: {s}")),
        Value::Bool(b) => Some(format!("{key}: {b}")),
        Value::Number(n) => Some(format!("{key}: {n}")),
        Value::Array(arr) => {
            let summary: Vec<String> = arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if summary.is_empty() { None } else { Some(format!("{key}: {}", summary.join(", "))) }
        }
        Value::Object(inner) => {
            let flat: Vec<String> = inner.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| format!("{k}: {s}")))
                .collect();
            if flat.is_empty() { None } else { Some(format!("{key}: {}", flat.join(", "))) }
        }
        Value::Null => None,
    }
}

fn format_object(obj: &serde_json::Map<String, Value>, indent: usize) -> String {
    obj.iter()
        .map(|(k, v)| format_object_entry(k, v, indent))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render one top-level `key: value` of an object as a single block.
fn format_object_entry(key: &str, val: &Value, indent: usize) -> String {
    let prefix = "  ".repeat(indent);
    match val {
        Value::String(s) => format!("{prefix}**{key}:** {s}"),
        Value::Bool(b) => format!("{prefix}**{key}:** {b}"),
        Value::Number(n) => format!("{prefix}**{key}:** {n}"),
        Value::Null => format!("{prefix}**{key}:** —"),
        Value::Array(arr) if arr.is_empty() => format!("{prefix}**{key}:** (empty)"),
        Value::Array(arr) => {
            let simple: Vec<String> = arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if simple.len() == arr.len() {
                format!("{prefix}**{key}:** {}", simple.join(", "))
            } else {
                let mut out = format!("{prefix}**{key}:**\n");
                out.push_str(&format_array(arr));
                out
            }
        }
        Value::Object(inner) => {
            let mut out = format!("{prefix}**{key}:**\n");
            out.push_str(&format_object(inner, indent + 1));
            out
        }
    }
}

/// Paginate a (possibly JSON) tool result, respecting structural
/// boundaries where possible:
/// - A JSON array splits between items (never mid-item).
/// - A JSON object splits between top-level keys.
/// - Anything else falls back to line-based `paginate_text`.
///
/// An individual block that exceeds `max_chars` is itself line-split as a
/// last resort.
pub fn paginate_tool_output(text: &str, max_chars: usize) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return paginate_text(text, max_chars);
    };
    match value {
        Value::Array(items) => {
            let mut blocks = Vec::with_capacity(items.len() + 1);
            blocks.push(format!("**{} items:**", items.len()));
            for (i, item) in items.iter().enumerate() {
                blocks.push(format_array_item(item, i));
            }
            pack_blocks(blocks, max_chars)
        }
        Value::Object(obj) => {
            let blocks: Vec<String> = obj.iter()
                .map(|(k, v)| format_object_entry(k, v, 0))
                .collect();
            pack_blocks(blocks, max_chars)
        }
        _ => paginate_text(&format_tool_output(text), max_chars),
    }
}

/// Pack rendered blocks into pages of at most `max_chars`. Never splits a
/// block internally unless the block is larger than `max_chars` on its
/// own, in which case that one block is line-split via `paginate_text`.
fn pack_blocks(blocks: Vec<String>, max_chars: usize) -> Vec<String> {
    let mut pages: Vec<String> = Vec::new();
    let mut current = String::new();
    for block in blocks {
        let sep = if current.is_empty() { 0 } else { 1 };
        if current.len() + sep + block.len() <= max_chars {
            if sep == 1 { current.push('\n'); }
            current.push_str(&block);
            continue;
        }
        if !current.is_empty() {
            pages.push(std::mem::take(&mut current));
        }
        if block.len() <= max_chars {
            current = block;
        } else {
            pages.extend(paginate_text(&block, max_chars));
        }
    }
    if !current.is_empty() { pages.push(current); }
    pages
}

/// Send a ToolService result to a Discord channel with the same formatting,
/// pagination, and attachment handling as `/tool call`. Shared between the
/// `/tool` command path and the voice session receiver.
///
/// Returns the posted message id of the primary message (the first one
/// carrying the embed or asset bundle) so callers can key stashed
/// replay state against it. `extra_files` are attached to whichever
/// single message this call emits (the asset-message branch, the error
/// branch, or the paginator's initial message) — callers can use this
/// for co-located sidecars if needed, though the canonical replay path
/// is now the `ToolStateStore` keyed by the returned id.
pub async fn render_tool_result_to_channel(
    http: &Arc<Http>,
    shard: &ShardMessenger,
    channel_id: ChannelId,
    tool_name: &str,
    result: anyhow::Result<Vec<ToolResultContent>>,
    extra_files: Vec<CreateAttachment>,
) -> anyhow::Result<u64> {
    let content = match result {
        Ok(c) => c,
        Err(e) => {
            let embed = CreateEmbed::new()
                .title(format!("Tool: {tool_name} (error)"))
                .description(format!("{e}"))
                .color(0xE74C3C);
            let mut msg = CreateMessage::new().embed(embed);
            for f in extra_files { msg = msg.add_file(f); }
            let posted = channel_id.send_message(http, msg).await?;
            return Ok(posted.id.get());
        }
    };

    let mut text_parts: Vec<&str> = Vec::new();
    let mut attachments: Vec<CreateAttachment> = Vec::new();
    for c in &content {
        match c {
            ToolResultContent::Text { text } => text_parts.push(text.as_str()),
            ToolResultContent::Image { data, mime_type } => {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                    let ext = mime_type.split('/').last().unwrap_or("png");
                    attachments.push(CreateAttachment::bytes(bytes, format!("asset.{ext}")));
                }
            }
            ToolResultContent::Audio { data, mime_type } => {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                    let ext = mime_type.split('/').last().unwrap_or("wav");
                    attachments.push(CreateAttachment::bytes(bytes, format!("audio.{ext}")));
                }
            }
        }
    }

    if !attachments.is_empty() {
        let mut msg = CreateMessage::new();
        for a in attachments { msg = msg.add_file(a); }
        for f in extra_files { msg = msg.add_file(f); }
        if !text_parts.is_empty() {
            msg = msg.content(text_parts.join("\n"));
        }
        let posted = channel_id.send_message(http, msg).await?;
        return Ok(posted.id.get());
    }

    let raw_text = text_parts.join("\n");
    let pages = paginate_tool_output(&raw_text, PAGE_MAX_CHARS);
    send_paginated_embeds_to_channel(
        http, shard, channel_id, tool_name, &pages, PAGINATION_TIMEOUT, extra_files,
    ).await
}
