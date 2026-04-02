//! AI chat message handling.
//!
//! Processes messages in AI Chats category channels and @mentions,
//! forwarding them to the daemon for LLM responses.

use serenity::builder::GetMessages;
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;
use simply_daemon::api::*;

use crate::commands::LuminaContext;

const DEFAULT_HISTORY_LIMIT: u16 = 1000;

/// Handle an incoming message — checks if it should get an AI response,
/// then processes it.
pub async fn handle_message(lx: &LuminaContext, msg: &Message) {
    if !should_respond(lx, msg) {
        return;
    }

    let typing = msg.channel_id.start_typing(&lx.http);

    if let Err(e) = process_chat(lx, msg).await {
        tracing::error!(error = %e, "chat processing failed");
        let _ = msg
            .channel_id
            .say(&lx.http, format!("Error: {e}"))
            .await;
    }

    drop(typing);
}

/// Core chat flow: load history, create session, send message, stream response.
async fn process_chat(lx: &LuminaContext, msg: &Message) -> anyhow::Result<()> {
    let bot_id = lx.ctx.cache.current_user().id;

    // 1. Load channel history as seed messages
    let limit = lx.config.discord.history_limit.unwrap_or(DEFAULT_HISTORY_LIMIT);
    let history = load_channel_history(lx, msg.channel_id, bot_id.get(), limit).await?;

    // 2. Create ephemeral session
    let (info, mut events) = lx
        .daemon
        .create_session(CreateSessionOptions {
            persistence: Some(Persistence::Ephemeral),
            ..Default::default()
        })
        .await?;

    // 3. Seed with channel history
    if !history.is_empty() {
        lx.daemon.seed_context(&info.id, history).await?;
    }

    // 4. Send the current message
    let user_text = format!("<@{}> says: {}", msg.author.id, msg.content);
    lx.daemon
        .send_message(
            &info.id,
            UserMessage {
                content: vec![InputContent::Text { text: user_text }],
                tool_filter: None,
            },
        )
        .await?;

    // 5. Stream response back to Discord
    stream_response(lx, msg, &info.id, &mut events).await?;

    // 6. Close session
    let _ = lx.daemon.close_session(&info.id).await;

    Ok(())
}

/// Load recent channel messages and convert to seed messages for the daemon.
async fn load_channel_history(
    lx: &LuminaContext,
    channel_id: ChannelId,
    bot_user_id: u64,
    limit: u16,
) -> anyhow::Result<Vec<SeedMessage>> {
    // Discord API caps at 100 per request, so we paginate
    let mut all_messages = Vec::new();
    let mut remaining = limit;
    let mut before = None;

    while remaining > 0 {
        let batch_size = remaining.min(100) as u8;
        let mut request = GetMessages::new().limit(batch_size);
        if let Some(id) = before {
            request = request.before(id);
        }

        let batch = channel_id.messages(&lx.http, request).await?;
        if batch.is_empty() {
            break;
        }

        before = batch.last().map(|m| m.id);
        remaining = remaining.saturating_sub(batch.len() as u16);
        all_messages.extend(batch);
    }

    let messages = all_messages;

    // Discord returns newest-first, we need oldest-first
    let mut seed: Vec<SeedMessage> = messages
        .iter()
        .rev()
        .filter(|m| !m.content.is_empty())
        .map(|m| {
            let role = if m.author.id.get() == bot_user_id {
                simply_daemon::api::types::Role::Assistant
            } else {
                simply_daemon::api::types::Role::User
            };

            // Prefix user messages with their Discord mention for attribution
            let text = if role == simply_daemon::api::types::Role::User {
                format!("<@{}> says: {}", m.author.id, m.content)
            } else {
                m.content.clone()
            };

            SeedMessage {
                role,
                content: vec![InputContent::Text { text }],
            }
        })
        .collect();

    // Don't include the current message (it'll be sent separately)
    if let Some(last) = seed.last() {
        if matches!(last.role, simply_daemon::api::types::Role::User) {
            seed.pop();
        }
    }

    Ok(seed)
}

/// Stream daemon events back to Discord with debounced message edits.
async fn stream_response(
    lx: &LuminaContext,
    msg: &Message,
    _session_id: &SessionId,
    events: &mut tokio::sync::broadcast::Receiver<DaemonEvent>,
) -> anyhow::Result<()> {
    let mut text_buffer = String::new();
    let mut discord_msg: Option<Message> = None;
    let mut last_edit = std::time::Instant::now();

    loop {
        match events.recv().await {
            Ok(DaemonEvent::TextDelta(delta)) => {
                text_buffer.push_str(&delta);

                // Debounced edit: update Discord message at most every 500ms
                if last_edit.elapsed() >= std::time::Duration::from_millis(500) {
                    let content = truncate_for_discord(&text_buffer);
                    match &mut discord_msg {
                        None => {
                            discord_msg =
                                Some(msg.channel_id.say(&lx.http, &content).await?);
                        }
                        Some(m) => {
                            m.edit(&lx.http, serenity::builder::EditMessage::new().content(&content))
                                .await?;
                        }
                    }
                    last_edit = std::time::Instant::now();
                }
            }
            Ok(DaemonEvent::TurnComplete) => {
                // Final flush
                if !text_buffer.is_empty() {
                    let content = truncate_for_discord(&text_buffer);
                    match &mut discord_msg {
                        None => {
                            msg.channel_id.say(&lx.http, &content).await?;
                        }
                        Some(m) => {
                            m.edit(&lx.http, serenity::builder::EditMessage::new().content(&content))
                                .await?;
                        }
                    }
                }
                break;
            }
            Ok(DaemonEvent::Error(e)) => {
                return Err(anyhow::anyhow!("daemon error: {e}"));
            }
            Ok(_) => {} // ToolCall, ToolResult, etc. — ignore for now
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(skipped = n, "event stream lagged");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }

    Ok(())
}

/// Truncate text to Discord's 2000 char limit.
fn truncate_for_discord(text: &str) -> String {
    if text.len() <= 2000 {
        text.to_string()
    } else {
        text[..2000].to_string()
    }
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Check if the bot should respond to this message.
fn should_respond(lx: &LuminaContext, msg: &Message) -> bool {
    is_ai_chat_channel(lx, msg) || is_mentioned(lx, msg)
}

/// Message is in a channel under the configured AI Chats category.
fn is_ai_chat_channel(lx: &LuminaContext, msg: &Message) -> bool {
    let cat_id = match lx.config.discord.ai_chats_category_id {
        Some(id) => ChannelId::new(id),
        None => return false,
    };

    let guild_id = match msg.guild_id {
        Some(id) => id,
        None => return false,
    };

    lx.ctx
        .cache
        .guild(guild_id)
        .and_then(|guild| {
            guild
                .channels
                .get(&msg.channel_id)
                .and_then(|ch| ch.parent_id)
                .map(|parent| parent == cat_id)
        })
        .unwrap_or(false)
}

/// Bot is @mentioned in the message.
fn is_mentioned(lx: &LuminaContext, msg: &Message) -> bool {
    let bot_id = lx.ctx.cache.current_user().id;
    msg.mentions.iter().any(|u| u.id == bot_id)
}
