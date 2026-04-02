//! Discord message paginator with navigation buttons.

use std::time::Duration;

use serenity::all::{
    ButtonStyle, CommandInteraction, ComponentInteractionDataKind, CreateActionRow, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse,
};
use serenity::futures::StreamExt;

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
