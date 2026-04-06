//! AI chat message handling.
//!
//! Processes messages in AI Chats category channels and @mentions,
//! forwarding them to the daemon for LLM responses.

use serenity::builder::{CreateEmbed, GetMessages};
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;
use simply_daemon::api::*;

use crate::commands::LuminaContext;

const DEFAULT_HISTORY_LIMIT: u16 = 1000;

// TODO: Load from UCM document (Content Stage 2)
const SYSTEM_PROMPT: &str = "\
You are Lumina, an intelligent AI assistant on Discord.

The messages in this conversation are your actual conversation history with the user(s) in this channel. You can refer back to anything said earlier — it is your memory of this conversation.

When formatting your response:
- For channel references: <#channel_id>
- For user mentions: <@user_id>
- For timestamps: <t:timestamp_seconds:R>

Messages from users are prefixed with their Discord mention (e.g. '<@12345> says: hello').
Use these mentions when referring to what someone said.

Be helpful, concise, and conversational.";

/// Handle an incoming message — checks if it should get an AI response,
/// then processes it.
pub async fn handle_message(lx: &LuminaContext, msg: &Message) {
    if !should_respond(lx, msg).await {
        tracing::debug!(
            channel_id = %msg.channel_id,
            author = %msg.author.name,
            "skipping message (not AI chat channel or mention)"
        );
        return;
    }

    tracing::info!(
        channel_id = %msg.channel_id,
        author = %msg.author.name,
        "processing chat message"
    );

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

    // 2. Create session with seed and resolved model
    let model_id = resolve_model(lx, msg).await;
    tracing::debug!(seed_count = history.len(), "creating session");
    let mut session = simply_daemon::DaemonSession::create(
        lx.daemon.clone(),
        CreateSessionOptions {
            persistence: Some(Persistence::Ephemeral),
            system_prompt: Some(SYSTEM_PROMPT.to_string()),
            model_id,
            seed: history,
        },
    )
    .await?;

    tracing::info!(session_id = %session.id(), model = %session.model_id(), "session created");

    // 3. Send the current message
    let user_text = format!("<@{}> says: {}", msg.author.id, msg.content);
    tracing::debug!("sending message to daemon");
    session
        .send(UserMessage {
            content: vec![InputContent::Text { text: user_text }],
        })
        .await?;

    // 4. Stream response back to Discord (session closes on drop)
    stream_response(lx, msg, &mut session).await
}

/// Load recent channel messages and convert to seed messages for the daemon.
async fn load_channel_history(
    lx: &LuminaContext,
    channel_id: ChannelId,
    bot_user_id: u64,
    limit: u16,
) -> anyhow::Result<Vec<SeedMessage>> {
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

    // Discord returns newest-first, we need oldest-first
    // Skip bot messages that appear before any user message (e.g. welcome messages)
    let mut seed: Vec<SeedMessage> = all_messages
        .iter()
        .rev()
        .filter(|m| !m.content.is_empty())
        .skip_while(|m| m.author.id.get() == bot_user_id) // skip leading bot messages
        .map(|m| {
            let role = if m.author.id.get() == bot_user_id {
                Role::Assistant
            } else {
                Role::User
            };

            let text = if role == Role::User {
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
        if matches!(last.role, Role::User) {
            seed.pop();
        }
    }

    Ok(seed)
}

/// Stream daemon events back to Discord.
/// Collects text via AssistantContent and sends once on TurnComplete.
async fn stream_response(
    lx: &LuminaContext,
    msg: &Message,
    session: &mut simply_daemon::DaemonSession,
) -> anyhow::Result<()> {
    let mut text_buffer = String::new();

    loop {
        match session.recv().await {
            Ok(DaemonEvent::TextDelta(_)) => {}
            Ok(DaemonEvent::AssistantContent(content_block)) => {
                match content_block {
                    ContentBlock::Text { text } => {
                        text_buffer.push_str(&text);
                    }
                    ContentBlock::Image { data, mime_type } => {
                        let ext = match mime_type.as_str() {
                            "image/png" => "png",
                            "image/gif" => "gif",
                            "image/webp" => "webp",
                            _ => "jpg",
                        };
                        if let Ok(bytes) = base64_decode(&data) {
                            let attachment = serenity::builder::CreateAttachment::bytes(bytes, format!("image.{ext}"));
                            msg.channel_id.send_message(&lx.http, serenity::builder::CreateMessage::new().add_file(attachment)).await?;
                        }
                    }
                    ContentBlock::Audio { data, mime_type } => {
                        let ext = match mime_type.as_str() {
                            "audio/mp3" | "audio/mpeg" => "mp3",
                            "audio/ogg" => "ogg",
                            "audio/wav" => "wav",
                            _ => "mp3",
                        };
                        if let Ok(bytes) = base64_decode(&data) {
                            let attachment = serenity::builder::CreateAttachment::bytes(bytes, format!("audio.{ext}"));
                            msg.channel_id.send_message(&lx.http, serenity::builder::CreateMessage::new().add_file(attachment)).await?;
                        }
                    }
                    _ => {}
                }
            }
            Ok(DaemonEvent::ToolCall { id: _, name, arguments }) => {
                let args_str = truncate_for_discord(&serde_json::to_string_pretty(&arguments).unwrap_or_default());
                let embed = CreateEmbed::new()
                    .title(format!("\u{1f527} Using: {name}"))
                    .description(format!("```json\n{args_str}\n```"))
                    .color(0x5865F2);
                msg.channel_id.send_message(&lx.http, serenity::builder::CreateMessage::new().embed(embed)).await?;
            }
            Ok(DaemonEvent::TurnComplete) => {
                if !text_buffer.is_empty() {
                    let content = truncate_for_discord(&text_buffer);
                    msg.channel_id.say(&lx.http, &content).await?;
                }
                break;
            }
            Ok(DaemonEvent::Error(e)) => {
                return Err(anyhow::anyhow!("daemon error: {e}"));
            }
            Ok(_) => {}
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

fn base64_decode(data: &str) -> anyhow::Result<Vec<u8>> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.decode(data)?)
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
// Model resolution
// ---------------------------------------------------------------------------

/// Resolve which model to use for a message.
/// Priority: channel topic tag → config default → None (daemon picks).
/// Validates the resolved model exists; falls back to config default if not.
async fn resolve_model(lx: &LuminaContext, msg: &Message) -> Option<String> {
    let from_topic = crate::commands::chat::get_topic_tag(lx, msg.channel_id, "model");

    let config_default = lx
        .config
        .discord
        .model_id
        .clone()
        .filter(|s| !s.is_empty());

    let candidate = from_topic.or(config_default.clone());

    let Some(model_id) = candidate else {
        return None;
    };

    // Validate the model exists
    if let Ok(models) = lx.daemon.model().list_models().await {
        let exists = models.iter().any(|m| m.id.to_string() == model_id);
        if exists {
            return Some(model_id);
        }
        tracing::warn!(model = %model_id, "model not found, falling back to default");
    }

    // Fallback to config default (which may also be None)
    config_default
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Check if the bot should respond to this message.
async fn should_respond(lx: &LuminaContext, msg: &Message) -> bool {
    if !is_ai_chat_channel(lx, msg) && !is_mentioned(lx, msg) {
        return false;
    }

    // Check channel topic for [paused] tag
    if crate::commands::chat::has_topic_tag(lx, msg.channel_id, "paused") {
        return false;
    }

    true
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
