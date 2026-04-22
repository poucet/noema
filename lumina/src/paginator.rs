//! Discord message paginator with navigation buttons.

use std::sync::Arc;
use std::time::Duration;

use serenity::all::{
    ButtonStyle, CommandInteraction, ComponentInteractionDataKind, CreateActionRow, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse,
    ShardMessenger,
};
use serenity::futures::StreamExt;
use serenity::http::Http;

use crate::commands::LuminaContext;

/// Send a paginated ephemeral message with prev/next buttons.
/// Pages are pre-built strings (each ≤ 2000 chars).
pub async fn send_paginated(
    lx: &LuminaContext,
    cmd: &CommandInteraction,
    pages: &[String],
    timeout: Duration,
) -> anyhow::Result<()> {
    if pages.is_empty() {
        cmd.create_response(
            &lx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("(empty)")
                    .ephemeral(true),
            ),
        )
        .await?;
        return Ok(());
    }

    if pages.len() == 1 {
        cmd.create_response(
            &lx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(&pages[0])
                    .ephemeral(true),
            ),
        )
        .await?;
        return Ok(());
    }

    let mut page = 0usize;

    cmd.create_response(
        &lx.http,
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content(format_page(&pages[page], page, pages.len()))
                .components(nav_buttons(page, pages.len()))
                .ephemeral(true),
        ),
    )
    .await?;

    let msg = cmd.get_response(&lx.http).await?;

    let mut collector = msg
        .await_component_interactions(&lx.ctx.shard)
        .timeout(timeout)
        .stream();

    while let Some(interaction) = collector.next().await {
        if let ComponentInteractionDataKind::Button = &interaction.data.kind {
            match interaction.data.custom_id.as_str() {
                "page_prev" => page = page.saturating_sub(1),
                "page_next" => page = (page + 1).min(pages.len() - 1),
                _ => continue,
            }

            interaction
                .create_response(
                    &lx.http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .content(format_page(&pages[page], page, pages.len()))
                            .components(nav_buttons(page, pages.len())),
                    ),
                )
                .await?;
        }
    }

    // Remove buttons after timeout
    cmd.edit_response(
        &lx.http,
        EditInteractionResponse::new()
            .content(format_page(&pages[page], page, pages.len()))
            .components(vec![]),
    )
    .await?;

    Ok(())
}

fn format_page(content: &str, page: usize, total: usize) -> String {
    format!("{content}\n\n*Page {}/{}*", page + 1, total)
}

fn nav_buttons(page: usize, total: usize) -> Vec<CreateActionRow> {
    vec![CreateActionRow::Buttons(vec![
        CreateButton::new("page_prev")
            .label("\u{25c0} Prev")
            .style(ButtonStyle::Secondary)
            .disabled(page == 0),
        CreateButton::new("page_next")
            .label("Next \u{25b6}")
            .style(ButtonStyle::Secondary)
            .disabled(page >= total - 1),
    ])]
}

/// A small "download the stashed tool JSON" button. `custom_id` is
/// fixed (`tool_json`); the handler looks up the payload by the
/// interaction's `message.id` so any tool-call / tool-result embed
/// carrying this button works without knowing the message id up front.
pub fn tool_json_button() -> CreateButton {
    CreateButton::new("tool_json")
        .style(ButtonStyle::Secondary)
        .label("JSON")
        .emoji(serenity::model::channel::ReactionType::Unicode("\u{1f4e5}".to_string()))
}

/// Send a paginated embed to a channel with prev/next buttons.
///
/// `extra_files` are attached to the initial message only; Discord preserves
/// them across edits triggered by page navigation, so they remain reachable
/// regardless of the page currently rendered.
pub async fn send_paginated_embeds_to_channel(
    http: &Arc<Http>,
    shard: &ShardMessenger,
    channel_id: serenity::model::id::ChannelId,
    title: &str,
    pages: &[String],
    timeout: Duration,
    extra_files: Vec<serenity::builder::CreateAttachment>,
) -> anyhow::Result<u64> {
    use serenity::builder::{CreateEmbed, CreateMessage, EditMessage};

    if pages.is_empty() {
        let embed = CreateEmbed::new().title(title).description("(empty)").color(0x2ECC71);
        let mut msg = CreateMessage::new().embed(embed);
        for f in extra_files {
            msg = msg.add_file(f);
        }
        let posted = channel_id.send_message(http, msg).await?;
        return Ok(posted.id.get());
    }

    let mut page = 0usize;
    let total = pages.len();

    let make_embed = |p: usize| {
        let mut e = CreateEmbed::new().description(&pages[p]).color(0x2ECC71);
        if total > 1 {
            e = e.title(format!("{title} ({}/{})", p + 1, total));
        } else {
            e = e.title(title);
        }
        e
    };

    // Combine pagination buttons (if any) with the JSON-download button.
    // Discord allows up to 5 buttons per ActionRow, so they fit together.
    let mut buttons: Vec<CreateButton> = if total > 1 {
        vec![
            CreateButton::new("page_prev")
                .label("\u{25c0} Prev")
                .style(ButtonStyle::Secondary)
                .disabled(page == 0),
            CreateButton::new("page_next")
                .label("Next \u{25b6}")
                .style(ButtonStyle::Secondary)
                .disabled(page >= total - 1),
        ]
    } else {
        Vec::new()
    };
    buttons.push(tool_json_button());
    let components = vec![CreateActionRow::Buttons(buttons)];
    let mut initial = CreateMessage::new().embed(make_embed(page)).components(components);
    for f in extra_files {
        initial = initial.add_file(f);
    }
    let msg = channel_id.send_message(http, initial).await?;
    let posted_id = msg.id.get();

    if total <= 1 {
        return Ok(posted_id);
    }

    // Filter to only pagination buttons — any other custom_id (notably
    // `tool_json`) falls through to the global interaction_create
    // handler in main.rs.
    let mut collector = msg
        .await_component_interactions(shard)
        .timeout(timeout)
        .filter(|i| {
            matches!(
                i.data.custom_id.as_str(),
                "page_prev" | "page_next"
            )
        })
        .stream();

    while let Some(interaction) = collector.next().await {
        if let ComponentInteractionDataKind::Button = &interaction.data.kind {
            match interaction.data.custom_id.as_str() {
                "page_prev" => page = page.saturating_sub(1),
                "page_next" => page = (page + 1).min(total - 1),
                // tool_json is handled by the global interaction dispatcher;
                // ignore here so this collector doesn't swallow it.
                _ => continue,
            }

            let row = CreateActionRow::Buttons(vec![
                CreateButton::new("page_prev")
                    .label("\u{25c0} Prev")
                    .style(ButtonStyle::Secondary)
                    .disabled(page == 0),
                CreateButton::new("page_next")
                    .label("Next \u{25b6}")
                    .style(ButtonStyle::Secondary)
                    .disabled(page >= total - 1),
                tool_json_button(),
            ]);
            interaction
                .create_response(
                    http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(make_embed(page))
                            .components(vec![row]),
                    ),
                )
                .await?;
        }
    }

    // After the pagination collector times out, keep only the download
    // button (pagination is useless on a frozen embed).
    msg.channel_id.edit_message(
        http,
        msg.id,
        EditMessage::new().embed(make_embed(page)).components(vec![
            CreateActionRow::Buttons(vec![tool_json_button()]),
        ]),
    ).await?;

    Ok(posted_id)
}

/// Split text into pages that fit within Discord's 2000 char limit.
/// Splits on newline boundaries.
pub fn paginate_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut pages = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        // +1 for the newline we'll add
        if !current.is_empty() && current.len() + line.len() + 1 > max_chars {
            pages.push(current);
            current = String::new();
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }

    if !current.is_empty() {
        pages.push(current);
    }

    pages
}
