//! AI chat message handling.
//!
//! Processes messages in AI Chats category channels and @mentions,
//! forwarding them to the daemon for LLM responses.

use serenity::model::channel::Message;
use serenity::model::id::ChannelId;

use crate::commands::LuminaContext;

/// Handle an incoming message — checks if it should get an AI response,
/// then processes it.
pub async fn handle_message(lx: &LuminaContext, msg: &Message) {
    if !should_respond(lx, msg) {
        return;
    }

    // Show typing indicator while processing
    let typing = msg.channel_id.start_typing(&lx.http);

    // TODO(2.3): Load channel history as conversation context
    // TODO(2.4): Open daemon session, send message, stream response

    // Echo placeholder until LLM integration is wired
    let reply = format!("*(echo)* {}", msg.content);
    if let Err(e) = msg.channel_id.say(&lx.http, &reply).await {
        tracing::error!(error = %e, "failed to send chat response");
    }

    drop(typing);
}

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

    // Look up channel in the guild cache to check parent category
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
