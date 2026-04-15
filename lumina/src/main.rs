//! Lumina — Discord bot for the Simply platform.
//!
//! Connects to Discord via serenity and to simply-daemon via WebSocket.

mod chat;
mod commands;
pub mod mcp;
pub mod paginator;
pub mod voice;

use std::sync::Arc;

use commands::CommandRegistry;
use serenity::model::id::{ChannelId, GuildId};
use serenity::prelude::*;
use songbird::SerenityInit;
use songbird::Config as SongbirdConfig;
use songbird::driver::{DecodeMode, DecodeConfig, Channels, SampleRate};
use simply_daemon::api::{Daemon, McpApi, RegisterEphemeralRequest};
use simply_daemon::net;

/// Key for storing the MCP server in serenity's TypeMap (to inject cache on ready).
pub struct McpServerKey;

impl TypeMapKey for McpServerKey {
    type Value = mcp::LuminaMcpServer;
}

/// Key for storing the daemon connection in serenity's TypeMap.
pub struct DaemonKey;

impl TypeMapKey for DaemonKey {
    type Value = Arc<dyn Daemon>;
}

/// Key for storing the Lumina config in serenity's TypeMap.
pub struct ConfigKey;

impl TypeMapKey for ConfigKey {
    type Value = config::LuminaConfig;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "lumina=info,simply_daemon=info".into());

    if let Ok(log_path) = std::env::var("LUMINA_LOG_FILE") {
        let file = std::fs::File::create(&log_path)?;
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(file)
            .with_ansi(false)
            .init();
        eprintln!("Logging to {log_path}");
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    };

    config::load_env_file();
    let lumina_cfg = config::LuminaConfig::load();
    let settings = config::Settings::load();

    let token = lumina_cfg
        .bot_token()
        .expect("DISCORD_BOT_TOKEN env var or discord.bot_token in lumina.toml is required");

    // Connect to existing daemon or start embedded
    let _daemon_handle = net::connect_or_host(
        settings.daemon_port,
        "lumina",
        None,
    ).await?;
    tracing::info!(host = _daemon_handle.is_host(), "daemon ready");

    let daemon = _daemon_handle.daemon();

    // Start Lumina's MCP server and register with daemon
    let http = Arc::new(serenity::http::Http::new(&token));
    let mcp_server = mcp::LuminaMcpServer::new(http);
    let _mcp_handle = simply_daemon::mcp::start_server(mcp_server.clone()).await?;
    let mcp_url = _mcp_handle.url();
    tracing::info!(url = %mcp_url, "lumina MCP server started");

    let tool_count = daemon
        .mcp().register_ephemeral_mcp(RegisterEphemeralRequest {
            id: "discord".to_string(),
            url: mcp_url,
        })
        .await?;
    tracing::info!(tool_count, "registered lumina MCP service with daemon");

    // Start Google Docs MCP server and register with daemon
    let _gdocs_handle = noema_mcp_gdocs::start_server().await?;
    let gdocs_url = _gdocs_handle.url();
    tracing::info!(url = %gdocs_url, "google docs MCP server started");

    let gdocs_tool_count = daemon
        .mcp().register_ephemeral_mcp(RegisterEphemeralRequest {
            id: "google-docs".to_string(),
            url: gdocs_url,
        })
        .await?;
    tracing::info!(gdocs_tool_count, "registered google docs MCP service with daemon");

    // Collect all auto-registered commands
    let registry = CommandRegistry::collect();
    tracing::info!(count = registry.len(), "commands registered");

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(token, intents)
        .event_handler(Handler {
            guild_ids: lumina_cfg.discord.guild_ids.iter().map(|&id| GuildId::new(id)).collect(),
            status_channel_id: lumina_cfg.discord.status_channel_id.map(ChannelId::new),
        })
        .register_songbird_from_config(
            SongbirdConfig::default().decode_mode(DecodeMode::Decode(
                DecodeConfig::new(Channels::Mono, SampleRate::Hz16000)
            ))
        )
        .type_map_insert::<DaemonKey>(daemon.clone())
        .type_map_insert::<voice::VoiceManagerKey>(Arc::new(voice::VoiceManager::new(daemon, lumina_cfg.voice.clone())))
        .type_map_insert::<ConfigKey>(lumina_cfg)
        .type_map_insert::<CommandRegistry>(registry)
        .type_map_insert::<commands::SharedState>(Arc::new(commands::SharedState::new()))
        .type_map_insert::<McpServerKey>(mcp_server)
        .await?;

    tracing::info!("lumina starting");
    client.start().await?;

    Ok(())
}

struct Handler {
    guild_ids: Vec<GuildId>,
    status_channel_id: Option<ChannelId>,
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: serenity::model::gateway::Ready) {
        tracing::info!(user = %ready.user.name, "connected to Discord — use .sync to register commands");

        // Inject the gateway cache into the MCP server so cache-dependent tools work
        {
            let data = ctx.data.read().await;
            if let Some(mcp) = data.get::<McpServerKey>() {
                mcp.set_cache(ctx.cache.clone());
                tracing::info!("injected gateway cache into lumina MCP server");
            }
        }

        if let Some(channel_id) = self.status_channel_id {
            // Purge last message, then post status — mirrors Python Lumina behavior
            if let Ok(messages) = channel_id.messages(&ctx.http, serenity::builder::GetMessages::new().limit(1)).await {
                for msg in messages {
                    let _ = msg.delete(&ctx.http).await;
                }
            }
            let _ = channel_id.say(
                &ctx.http,
                format!("\u{1f7e2} Bot connected as <@{}>", ready.user.id),
            ).await;
        }
    }

    async fn cache_ready(&self, ctx: Context, _guilds: Vec<GuildId>) {
        // Cache is fully populated — refresh channel map in MCP instructions
        let data = ctx.data.read().await;
        if let Some(mcp) = data.get::<McpServerKey>() {
            mcp.refresh_instructions();
            tracing::info!("refreshed MCP instructions with channel map");
        }
    }

    async fn channel_create(&self, ctx: Context, _channel: serenity::model::channel::GuildChannel) {
        let data = ctx.data.read().await;
        if let Some(mcp) = data.get::<McpServerKey>() { mcp.refresh_instructions(); }
    }

    async fn channel_delete(&self, ctx: Context, _channel: serenity::model::channel::GuildChannel, _messages: Option<Vec<serenity::model::channel::Message>>) {
        let data = ctx.data.read().await;
        if let Some(mcp) = data.get::<McpServerKey>() { mcp.refresh_instructions(); }
    }

    async fn channel_update(&self, ctx: Context, _old: Option<serenity::model::channel::GuildChannel>, _new: serenity::model::channel::GuildChannel) {
        let data = ctx.data.read().await;
        if let Some(mcp) = data.get::<McpServerKey>() { mcp.refresh_instructions(); }
    }

    async fn message(&self, ctx: Context, msg: serenity::model::channel::Message) {
        if msg.author.bot {
            return;
        }

        // Owner-only prefix commands
        if msg.content.starts_with('.') {
            let data = ctx.data.read().await;
            let cfg = data.get::<ConfigKey>().expect("ConfigKey missing");
            let is_owner = cfg.discord.owner_id.map_or(false, |id| msg.author.id.get() == id);
            if is_owner {
                match msg.content.as_str() {
                    ".sync" => {
                        let registry = data.get::<CommandRegistry>().expect("CommandRegistry missing");
                        let definitions = registry.definitions();
                        drop(data);

                        let mut ok = 0usize;
                        let mut fail = 0usize;
                        for &guild_id in &self.guild_ids {
                            match guild_id.set_commands(&ctx.http, definitions.clone()).await {
                                Ok(_) => ok += 1,
                                Err(e) => {
                                    tracing::error!(guild_id = %guild_id, error = %e, "sync failed");
                                    fail += 1;
                                }
                            }
                        }
                        let _ = msg.reply(&ctx.http, format!("Synced commands to {ok} guild(s), {fail} failed.")).await;
                    }
                    _ => {}
                }
                return;
            }
        }

        // AI chat: delegate to chat module for detection + response
        let lx = commands::LuminaContext::from_serenity(&ctx).await;
        chat::handle_message(&lx, &msg).await;
    }

    async fn interaction_create(&self, ctx: Context, interaction: serenity::model::application::Interaction) {
        match interaction {
            serenity::model::application::Interaction::Command(cmd) => {
                let lx = commands::LuminaContext::from_serenity(&ctx).await;
                let data = ctx.data.read().await;
                let registry = data.get::<CommandRegistry>().expect("CommandRegistry missing");
                registry.dispatch(&lx, &cmd).await;
            }
            serenity::model::application::Interaction::Autocomplete(ac) => {
                let lx = commands::LuminaContext::from_serenity(&ctx).await;
                let data = ctx.data.read().await;
                let registry = data.get::<CommandRegistry>().expect("CommandRegistry missing");
                registry.dispatch_autocomplete(&lx, &ac).await;
            }
            serenity::model::application::Interaction::Modal(modal) => {
                if modal.data.custom_id.starts_with("tool_call:") {
                    let lx = commands::LuminaContext::from_serenity(&ctx).await;
                    if let Err(e) = commands::tool::handle_modal(&lx, &modal).await {
                        tracing::error!(error = %e, "tool modal failed");
                    }
                }
            }
            _ => {}
        }
    }
}
