//! Discord MCP tool definitions and handlers.

use super::{tool, LuminaMcpServer};
use anyhow::{Context as _, Result};
use rmcp::model::Tool;
use serde::Deserialize;
use serde_json::{json, Value};
use serenity::builder::{CreateEmbed, CreateMessage, GetMessages};
use serenity::model::id::{ChannelId, GuildId, MessageId};
use std::collections::HashMap;

type Map = serde_json::Map<String, Value>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse and deserialize tool arguments.
fn parse<T: serde::de::DeserializeOwned>(args: Map) -> Result<T> {
    serde_json::from_value(Value::Object(args)).context("invalid arguments")
}

/// Discord epoch (2015-01-01T00:00:00Z) in milliseconds.
const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

/// Create a snowflake ID representing `hours` ago (for history filtering).
fn snowflake_hours_ago(hours: u64) -> MessageId {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let target_ms = now_ms.saturating_sub(hours * 3_600_000);
    MessageId::new((target_ms.saturating_sub(DISCORD_EPOCH_MS)) << 22)
}

/// Fetch messages from a channel after a given message ID, paginating up to `max`.
async fn fetch_messages_since(
    http: &serenity::http::Http,
    channel_id: ChannelId,
    after: MessageId,
    max: usize,
) -> Result<Vec<serenity::model::channel::Message>> {
    let mut all = Vec::new();
    let mut cursor = after;

    loop {
        let batch = channel_id
            .messages(http, GetMessages::new().after(cursor).limit(100))
            .await?;
        if batch.is_empty() {
            break;
        }
        // after() returns oldest-first; last element is the newest
        cursor = batch.iter().map(|m| m.id).max().unwrap_or(cursor);
        all.extend(batch);
        if all.len() >= max {
            all.truncate(max);
            break;
        }
    }

    Ok(all)
}

/// Build an optional embed from common embed parameters.
fn build_embed(title: Option<&str>, description: Option<&str>, color: Option<&str>) -> Option<CreateEmbed> {
    if title.is_none() && description.is_none() {
        return None;
    }
    let mut embed = CreateEmbed::new();
    if let Some(t) = title {
        embed = embed.title(t);
    }
    if let Some(d) = description {
        embed = embed.description(d);
    }
    if let Some(hex) = color {
        if let Ok(c) = u32::from_str_radix(hex.trim_start_matches('#'), 16) {
            embed = embed.color(c);
        }
    }
    Some(embed)
}

/// Serialize a serenity Message to a JSON value (subset of fields).
fn message_to_json(msg: &serenity::model::channel::Message) -> Value {
    json!({
        "id": msg.id.get(),
        "channel_id": msg.channel_id.get(),
        "author": { "id": msg.author.id.get(), "name": msg.author.name },
        "content": msg.content,
        "timestamp": msg.timestamp.to_string(),
        "reactions": msg.reactions.iter().map(|r| json!({
            "emoji": r.reaction_type.to_string(),
            "count": r.count,
        })).collect::<Vec<_>>(),
        "attachments": msg.attachments.len(),
        "embeds": msg.embeds.len(),
    })
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

pub(super) fn definitions() -> Vec<Tool> {
    vec![
        // -- Messaging --
        tool!("send_message", "Send a message to a Discord channel",
            required: ["channel_id", "content"],
            {
                "channel_id": { "type": "integer", "description": "Discord channel ID" },
                "content": { "type": "string", "description": "Message content" },
                "embed_title": { "type": "string", "description": "Optional embed title" },
                "embed_description": { "type": "string", "description": "Optional embed description" },
                "embed_color": { "type": "string", "description": "Hex color code (e.g. #FF0000)" }
            }
        ),
        tool!("reply_to_message", "Reply to a specific message in a Discord channel",
            required: ["channel_id", "message_id", "content"],
            {
                "channel_id": { "type": "integer", "description": "Discord channel ID" },
                "message_id": { "type": "integer", "description": "ID of the message to reply to" },
                "content": { "type": "string", "description": "Reply content" },
                "mention_author": { "type": "boolean", "description": "Whether to mention the original author (default true)" },
                "embed_title": { "type": "string", "description": "Optional embed title" },
                "embed_description": { "type": "string", "description": "Optional embed description" },
                "embed_color": { "type": "string", "description": "Hex color code (e.g. #FF0000)" }
            }
        ),
        tool!("create_embed", "Send a rich embed message to a Discord channel",
            required: ["channel_id", "title", "description"],
            {
                "channel_id": { "type": "integer", "description": "Discord channel ID" },
                "title": { "type": "string", "description": "Embed title" },
                "description": { "type": "string", "description": "Embed description" },
                "fields_names": { "type": "array", "items": { "type": "string" }, "description": "Field names" },
                "fields_values": { "type": "array", "items": { "type": "string" }, "description": "Field values" }
            }
        ),
        tool!("create_poll", "Create a reaction-based poll in a Discord channel",
            required: ["channel_id", "question", "options"],
            {
                "channel_id": { "type": "integer", "description": "Discord channel ID" },
                "question": { "type": "string", "description": "Poll question" },
                "options": { "type": "array", "items": { "type": "string" }, "description": "Poll options (max 10)" },
                "emojis": { "type": "array", "items": { "type": "string" }, "description": "Custom emojis for options" }
            }
        ),
        // -- Read --
        tool!("get_channel_history", "Get message history from a Discord channel",
            required: ["channel_id"],
            {
                "channel_id": { "type": "integer", "description": "Discord channel ID" },
                "limit": { "type": "integer", "description": "Max messages to fetch (default 100, max 1000)" }
            }
        ),
        tool!("search_messages", "Search for messages in a channel containing specific text",
            required: ["channel_id", "query"],
            {
                "channel_id": { "type": "integer", "description": "Discord channel ID" },
                "query": { "type": "string", "description": "Text to search for (case-insensitive)" },
                "limit": { "type": "integer", "description": "Max messages to scan (default 100)" }
            }
        ),
        tool!("list_channels", "List all channels in a guild",
            required: ["guild_id"],
            {
                "guild_id": { "type": "integer", "description": "Discord guild (server) ID" }
            }
        ),
        tool!("list_guilds", "List all guilds the bot is in", required: [], {}),
        tool!("get_emoji_list", "Get custom emojis in a guild",
            required: ["guild_id"],
            {
                "guild_id": { "type": "integer", "description": "Discord guild ID" }
            }
        ),
        tool!("get_voice_states", "Get users currently in voice channels (requires active gateway)",
            required: ["guild_id"],
            {
                "guild_id": { "type": "integer", "description": "Discord guild ID" }
            }
        ),
        // -- Analytics --
        tool!("get_message_stats", "Get message statistics for a channel over a time period",
            required: ["channel_id"],
            {
                "channel_id": { "type": "integer", "description": "Discord channel ID" },
                "hours": { "type": "integer", "description": "Time period in hours (default 24)" }
            }
        ),
        tool!("get_channel_peak_hours", "Get the most active hours for a channel",
            required: ["channel_id"],
            {
                "channel_id": { "type": "integer", "description": "Discord channel ID" },
                "days": { "type": "integer", "description": "Time period in days (default 7)" }
            }
        ),
        tool!("get_trending_content", "Get trending messages and reactions in a guild",
            required: ["guild_id"],
            {
                "guild_id": { "type": "integer", "description": "Discord guild ID" },
                "hours": { "type": "integer", "description": "Time period in hours (default 24)" },
                "min_reactions": { "type": "integer", "description": "Minimum reactions to count as trending (default 1)" }
            }
        ),
        tool!("get_active_threads", "Get active threads in a guild",
            required: ["guild_id"],
            {
                "guild_id": { "type": "integer", "description": "Discord guild ID" }
            }
        ),
        tool!("get_user_activity", "Get activity statistics for a specific user in a guild",
            required: ["guild_id", "user_id"],
            {
                "guild_id": { "type": "integer", "description": "Discord guild ID" },
                "user_id": { "type": "integer", "description": "Discord user ID" },
                "days": { "type": "integer", "description": "Time period in days (default 7)" }
            }
        ),
    ]
}

// ---------------------------------------------------------------------------
// Argument structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SendMessageArgs {
    channel_id: u64,
    content: String,
    embed_title: Option<String>,
    embed_description: Option<String>,
    embed_color: Option<String>,
}

#[derive(Deserialize)]
struct ReplyToMessageArgs {
    channel_id: u64,
    message_id: u64,
    content: String,
    #[serde(default = "default_true")]
    mention_author: bool,
    embed_title: Option<String>,
    embed_description: Option<String>,
    embed_color: Option<String>,
}

fn default_true() -> bool { true }

#[derive(Deserialize)]
struct CreateEmbedArgs {
    channel_id: u64,
    title: String,
    description: String,
    fields_names: Option<Vec<String>>,
    fields_values: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct CreatePollArgs {
    channel_id: u64,
    question: String,
    options: Vec<String>,
    emojis: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ChannelHistoryArgs {
    channel_id: u64,
    limit: Option<u64>,
}

#[derive(Deserialize)]
struct SearchMessagesArgs {
    channel_id: u64,
    query: String,
    limit: Option<u64>,
}

#[derive(Deserialize)]
struct GuildIdArgs {
    guild_id: u64,
}

#[derive(Deserialize)]
struct ChannelStatsArgs {
    channel_id: u64,
    hours: Option<u64>,
}

#[derive(Deserialize)]
struct ChannelPeakArgs {
    channel_id: u64,
    days: Option<u64>,
}

#[derive(Deserialize)]
struct TrendingArgs {
    guild_id: u64,
    hours: Option<u64>,
    min_reactions: Option<u64>,
}

#[derive(Deserialize)]
struct UserActivityArgs {
    guild_id: u64,
    user_id: u64,
    days: Option<u64>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

const NUMBER_EMOJIS: &[&str] = &["1️⃣", "2️⃣", "3️⃣", "4️⃣", "5️⃣", "6️⃣", "7️⃣", "8️⃣", "9️⃣", "🔟"];

impl LuminaMcpServer {
    // -- Messaging --

    pub(super) async fn handle_send_message(&self, args: Map) -> Result<Value> {
        let a: SendMessageArgs = parse(args)?;
        let channel = ChannelId::new(a.channel_id);

        let mut msg = CreateMessage::new().content(&a.content);
        if let Some(embed) = build_embed(
            a.embed_title.as_deref(),
            a.embed_description.as_deref(),
            a.embed_color.as_deref(),
        ) {
            msg = msg.embed(embed);
        }

        let sent = channel.send_message(self.http(), msg).await?;
        Ok(json!({
            "status": "success",
            "message_id": sent.id.get(),
            "channel_id": sent.channel_id.get(),
            "jump_url": format!("https://discord.com/channels/{}/{}/{}",
                sent.guild_id.map(|g| g.get()).unwrap_or(0),
                sent.channel_id.get(), sent.id.get()),
        }))
    }

    pub(super) async fn handle_reply_to_message(&self, args: Map) -> Result<Value> {
        let a: ReplyToMessageArgs = parse(args)?;
        let channel = ChannelId::new(a.channel_id);
        let ref_msg = channel.message(self.http(), MessageId::new(a.message_id)).await?;

        let mut msg = CreateMessage::new().content(&a.content);
        if let Some(embed) = build_embed(
            a.embed_title.as_deref(),
            a.embed_description.as_deref(),
            a.embed_color.as_deref(),
        ) {
            msg = msg.embed(embed);
        }
        msg = msg.reference_message(&ref_msg);
        if !a.mention_author {
            msg = msg.allowed_mentions(
                serenity::builder::CreateAllowedMentions::new()
                    .replied_user(false),
            );
        }

        let sent = channel.send_message(self.http(), msg).await?;
        Ok(json!({
            "status": "success",
            "message_id": sent.id.get(),
            "reference_message_id": a.message_id,
        }))
    }

    pub(super) async fn handle_create_embed(&self, args: Map) -> Result<Value> {
        let a: CreateEmbedArgs = parse(args)?;
        let channel = ChannelId::new(a.channel_id);

        let mut embed = CreateEmbed::new()
            .title(&a.title)
            .description(&a.description)
            .color(0x1ABC9C); // teal

        if let (Some(names), Some(values)) = (&a.fields_names, &a.fields_values) {
            for (name, value) in names.iter().zip(values.iter()) {
                let v = if value.len() > 1024 {
                    format!("{}...", &value[..1021])
                } else {
                    value.clone()
                };
                embed = embed.field(name, v, false);
            }
        }

        let sent = channel
            .send_message(self.http(), CreateMessage::new().embed(embed))
            .await?;
        Ok(json!({ "status": "success", "message_id": sent.id.get() }))
    }

    pub(super) async fn handle_create_poll(&self, args: Map) -> Result<Value> {
        let a: CreatePollArgs = parse(args)?;
        if a.options.len() > 10 {
            anyhow::bail!("maximum 10 poll options");
        }

        let channel = ChannelId::new(a.channel_id);
        let emojis = a.emojis.as_deref().unwrap_or(&[]);

        let options_text: Vec<String> = a
            .options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let emoji = emojis
                    .get(i)
                    .map(|s| s.as_str())
                    .unwrap_or(NUMBER_EMOJIS.get(i).copied().unwrap_or("▪️"));
                format!("{emoji} - {opt}")
            })
            .collect();

        let embed = CreateEmbed::new()
            .title(format!("📋 Poll: {}", a.question))
            .description(options_text.join("\n"))
            .color(0x3498DB);

        let sent = channel
            .send_message(self.http(), CreateMessage::new().embed(embed))
            .await?;

        // Add reaction emojis
        for (i, _) in a.options.iter().enumerate() {
            let emoji_str = emojis
                .get(i)
                .map(|s| s.as_str())
                .unwrap_or(NUMBER_EMOJIS.get(i).copied().unwrap_or("▪️"));
            let reaction = serenity::model::channel::ReactionType::Unicode(emoji_str.to_string());
            sent.react(self.http(), reaction).await?;
        }

        Ok(json!({
            "status": "success",
            "message_id": sent.id.get(),
            "question": a.question,
            "options_count": a.options.len(),
        }))
    }

    // -- Read --

    pub(super) async fn handle_get_channel_history(&self, args: Map) -> Result<Value> {
        let a: ChannelHistoryArgs = parse(args)?;
        let channel = ChannelId::new(a.channel_id);
        let limit = a.limit.unwrap_or(100).min(1000);

        let mut all = Vec::new();
        let mut before: Option<MessageId> = None;
        let mut remaining = limit;

        while remaining > 0 {
            let batch_size = remaining.min(100) as u8;
            let mut req = GetMessages::new().limit(batch_size);
            if let Some(id) = before {
                req = req.before(id);
            }
            let batch = channel.messages(self.http(), req).await?;
            if batch.is_empty() {
                break;
            }
            before = batch.last().map(|m| m.id);
            remaining = remaining.saturating_sub(batch.len() as u64);
            all.extend(batch);
        }

        let messages: Vec<Value> = all.iter().map(message_to_json).collect();
        Ok(json!({ "status": "success", "messages": messages, "count": messages.len() }))
    }

    pub(super) async fn handle_search_messages(&self, args: Map) -> Result<Value> {
        let a: SearchMessagesArgs = parse(args)?;
        let channel = ChannelId::new(a.channel_id);
        let limit = a.limit.unwrap_or(100).min(1000);
        let query_lower = a.query.to_lowercase();

        let mut all = Vec::new();
        let mut before: Option<MessageId> = None;
        let mut remaining = limit;
        let mut matches = Vec::new();

        while remaining > 0 {
            let batch_size = remaining.min(100) as u8;
            let mut req = GetMessages::new().limit(batch_size);
            if let Some(id) = before {
                req = req.before(id);
            }
            let batch = channel.messages(self.http(), req).await?;
            if batch.is_empty() {
                break;
            }
            before = batch.last().map(|m| m.id);
            remaining = remaining.saturating_sub(batch.len() as u64);

            for msg in &batch {
                if msg.content.to_lowercase().contains(&query_lower) {
                    matches.push(message_to_json(msg));
                }
            }
            all.extend(batch);
        }

        Ok(json!({ "status": "success", "messages": matches, "count": matches.len() }))
    }

    pub(super) async fn handle_list_channels(&self, args: Map) -> Result<Value> {
        let a: GuildIdArgs = parse(args)?;
        let guild = GuildId::new(a.guild_id);
        let channels = guild.channels(self.http()).await?;

        let list: Vec<Value> = channels
            .values()
            .map(|ch| {
                json!({
                    "name": ch.name,
                    "id": ch.id.get(),
                    "kind": format!("{:?}", ch.kind),
                    "parent_id": ch.parent_id.map(|p| p.get()),
                })
            })
            .collect();

        Ok(json!({ "status": "success", "channels": list, "count": list.len() }))
    }

    pub(super) async fn handle_list_guilds(&self, _args: Map) -> Result<Value> {
        let cache = self.cache().context("gateway cache not available yet")?;
        let guilds: Vec<Value> = cache
            .guilds()
            .iter()
            .filter_map(|id| {
                cache.guild(*id).map(|g| {
                    json!({ "name": g.name.clone(), "id": id.get() })
                })
            })
            .collect();
        Ok(json!({ "status": "success", "guilds": guilds, "count": guilds.len() }))
    }

    pub(super) async fn handle_get_emoji_list(&self, args: Map) -> Result<Value> {
        let a: GuildIdArgs = parse(args)?;
        let guild = GuildId::new(a.guild_id);
        let emojis = guild.emojis(self.http()).await?;

        let list: Vec<Value> = emojis
            .iter()
            .map(|e| {
                json!({
                    "name": e.name,
                    "id": e.id.get(),
                    "animated": e.animated,
                    "available": e.available,
                })
            })
            .collect();

        Ok(json!({ "status": "success", "emojis": list, "count": list.len() }))
    }

    pub(super) async fn handle_get_voice_states(&self, args: Map) -> Result<Value> {
        let a: GuildIdArgs = parse(args)?;
        let cache = self.cache().context("gateway cache not available yet")?;
        let guild = cache
            .guild(GuildId::new(a.guild_id))
            .context("guild not in cache")?;

        let mut voice_channels: HashMap<ChannelId, Vec<Value>> = HashMap::new();
        for (user_id, vs) in &guild.voice_states {
            if let Some(channel_id) = vs.channel_id {
                voice_channels.entry(channel_id).or_default().push(json!({
                    "user_id": user_id.get(),
                    "self_mute": vs.self_mute,
                    "self_deaf": vs.self_deaf,
                    "mute": vs.mute,
                    "deaf": vs.deaf,
                }));
            }
        }

        let states: Vec<Value> = voice_channels
            .into_iter()
            .map(|(ch_id, members)| {
                let ch_name = guild
                    .channels
                    .get(&ch_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                json!({
                    "channel_name": ch_name,
                    "channel_id": ch_id.get(),
                    "members": members,
                    "member_count": members.len(),
                })
            })
            .collect();

        let total: usize = states.iter().filter_map(|s| s["member_count"].as_u64()).sum::<u64>() as usize;
        Ok(json!({ "status": "success", "voice_states": states, "total_voice_users": total }))
    }

    // -- Analytics --

    pub(super) async fn handle_get_message_stats(&self, args: Map) -> Result<Value> {
        let a: ChannelStatsArgs = parse(args)?;
        let channel = ChannelId::new(a.channel_id);
        let hours = a.hours.unwrap_or(24);
        let after = snowflake_hours_ago(hours);

        let messages = fetch_messages_since(self.http(), channel, after, 10_000).await?;

        let mut unique_authors = std::collections::HashSet::new();
        let mut reactions: u64 = 0;
        let mut attachments: u64 = 0;
        let mut embeds: u64 = 0;

        for msg in &messages {
            unique_authors.insert(msg.author.id);
            reactions += msg.reactions.iter().map(|r| r.count).sum::<u64>();
            attachments += msg.attachments.len() as u64;
            embeds += msg.embeds.len() as u64;
        }

        Ok(json!({
            "status": "success",
            "stats": {
                "time_period_hours": hours,
                "total_messages": messages.len(),
                "unique_authors": unique_authors.len(),
                "reactions": reactions,
                "attachments": attachments,
                "embeds": embeds,
                "messages_per_hour": if hours > 0 { messages.len() as f64 / hours as f64 } else { 0.0 },
            }
        }))
    }

    pub(super) async fn handle_get_channel_peak_hours(&self, args: Map) -> Result<Value> {
        let a: ChannelPeakArgs = parse(args)?;
        let channel = ChannelId::new(a.channel_id);
        let days = a.days.unwrap_or(7);
        let after = snowflake_hours_ago(days * 24);

        let messages = fetch_messages_since(self.http(), channel, after, 50_000).await?;

        let mut hourly: HashMap<u32, u64> = HashMap::new();
        let mut daily: HashMap<String, u64> = HashMap::new();

        for msg in &messages {
            let ts = msg.timestamp;
            // Extract hour and weekday from the timestamp
            let hour = (ts.unix_timestamp() % 86400 / 3600) as u32;
            let weekday = match (ts.unix_timestamp() / 86400 + 4) % 7 {
                0 => "Sunday", 1 => "Monday", 2 => "Tuesday", 3 => "Wednesday",
                4 => "Thursday", 5 => "Friday", _ => "Saturday",
            };
            *hourly.entry(hour).or_default() += 1;
            *daily.entry(weekday.to_string()).or_default() += 1;
        }

        let mut hourly_stats: Vec<Value> = hourly
            .iter()
            .map(|(&h, &c)| json!({"hour": h, "messages": c}))
            .collect();
        hourly_stats.sort_by_key(|v| v["hour"].as_u64().unwrap_or(0));

        let avg = if days > 0 { messages.len() as f64 / (24.0 * days as f64) } else { 0.0 };
        let peak_hours: Vec<u32> = hourly
            .iter()
            .filter(|(_, &count)| (count as f64 / days as f64) > avg)
            .map(|(&hour, _)| hour)
            .collect();

        Ok(json!({
            "status": "success",
            "stats": {
                "time_period_days": days,
                "total_messages": messages.len(),
                "hourly_activity": hourly_stats,
                "daily_activity": daily,
                "peak_hours": peak_hours,
            }
        }))
    }

    pub(super) async fn handle_get_trending_content(&self, args: Map) -> Result<Value> {
        let a: TrendingArgs = parse(args)?;
        let cache = self.cache().context("gateway cache not available yet")?;
        let guild_ref = cache
            .guild(GuildId::new(a.guild_id))
            .context("guild not in cache")?;
        let text_channels: Vec<ChannelId> = guild_ref
            .channels
            .values()
            .filter(|ch| ch.kind == serenity::model::channel::ChannelType::Text)
            .map(|ch| ch.id)
            .collect();
        drop(guild_ref);

        let hours = a.hours.unwrap_or(24);
        let min_reactions = a.min_reactions.unwrap_or(1);
        let after = snowflake_hours_ago(hours);

        let mut trending = Vec::new();
        let mut emoji_counts: HashMap<String, u64> = HashMap::new();

        for ch_id in text_channels {
            let messages = fetch_messages_since(self.http(), ch_id, after, 1000).await.unwrap_or_default();
            for msg in &messages {
                let total_reactions: u64 = msg.reactions.iter().map(|r| r.count).sum();
                for r in &msg.reactions {
                    *emoji_counts.entry(r.reaction_type.to_string()).or_default() += r.count;
                }
                if total_reactions >= min_reactions {
                    trending.push(json!({
                        "content": if msg.content.len() > 200 { format!("{}...", &msg.content[..197]) } else { msg.content.clone() },
                        "author": msg.author.name,
                        "channel_id": msg.channel_id.get(),
                        "reactions": total_reactions,
                        "timestamp": msg.timestamp.to_string(),
                    }));
                }
            }
        }

        trending.sort_by(|a, b| {
            b["reactions"].as_u64().unwrap_or(0).cmp(&a["reactions"].as_u64().unwrap_or(0))
        });
        trending.truncate(10);

        let mut popular_emojis: Vec<Value> = emoji_counts
            .iter()
            .map(|(e, c)| json!({"emoji": e, "count": c}))
            .collect();
        popular_emojis.sort_by(|a, b| b["count"].as_u64().cmp(&a["count"].as_u64()));
        popular_emojis.truncate(10);

        Ok(json!({
            "status": "success",
            "trending": {
                "time_period_hours": hours,
                "messages": trending,
                "popular_emojis": popular_emojis,
                "total_trending_messages": trending.len(),
            }
        }))
    }

    pub(super) async fn handle_get_active_threads(&self, args: Map) -> Result<Value> {
        let a: GuildIdArgs = parse(args)?;
        let guild = GuildId::new(a.guild_id);
        let threads_data = guild.get_active_threads(self.http()).await?;

        let threads: Vec<Value> = threads_data
            .threads
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "id": t.id.get(),
                    "parent_id": t.parent_id.map(|p| p.get()),
                    "member_count": t.member_count,
                    "message_count": t.message_count,
                    "owner_id": t.owner_id.map(|o| o.get()),
                })
            })
            .collect();

        Ok(json!({ "status": "success", "threads": threads, "count": threads.len() }))
    }

    pub(super) async fn handle_get_user_activity(&self, args: Map) -> Result<Value> {
        let a: UserActivityArgs = parse(args)?;
        let cache = self.cache().context("gateway cache not available yet")?;
        let guild_ref = cache
            .guild(GuildId::new(a.guild_id))
            .context("guild not in cache")?;
        let text_channels: Vec<ChannelId> = guild_ref
            .channels
            .values()
            .filter(|ch| ch.kind == serenity::model::channel::ChannelType::Text)
            .map(|ch| ch.id)
            .collect();
        let member_name = guild_ref
            .members
            .get(&serenity::model::id::UserId::new(a.user_id))
            .map(|m| m.display_name().to_string())
            .unwrap_or_else(|| format!("User {}", a.user_id));
        drop(guild_ref);

        let days = a.days.unwrap_or(7);
        let after = snowflake_hours_ago(days * 24);
        let target_user = serenity::model::id::UserId::new(a.user_id);

        let mut total_messages: u64 = 0;
        let mut channel_activity: HashMap<String, u64> = HashMap::new();

        for ch_id in text_channels {
            let messages = fetch_messages_since(self.http(), ch_id, after, 5000)
                .await
                .unwrap_or_default();
            let count = messages.iter().filter(|m| m.author.id == target_user).count() as u64;
            if count > 0 {
                // Get channel name from cache if available
                let ch_name = self
                    .cache()
                    .and_then(|c| c.guild(GuildId::new(a.guild_id)))
                    .and_then(|g| g.channels.get(&ch_id).map(|c| c.name.clone()))
                    .unwrap_or_else(|| ch_id.get().to_string());
                channel_activity.insert(ch_name, count);
                total_messages += count;
            }
        }

        Ok(json!({
            "status": "success",
            "activity": {
                "user_name": member_name,
                "time_period_days": days,
                "total_messages": total_messages,
                "messages_per_day": if days > 0 { total_messages as f64 / days as f64 } else { 0.0 },
                "channel_activity": channel_activity,
            }
        }))
    }
}
