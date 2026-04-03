//! Lumina — Discord bot for the Simply platform.
//!
//! Connects to Discord via serenity and to simply-daemon via WebSocket.

mod chat;
mod commands;
pub mod mcp;
pub mod paginator;

use std::sync::Arc;

use commands::CommandRegistry;
use serenity::model::id::{ChannelId, GuildId};
use serenity::prelude::*;
use simply_daemon::api::{DaemonApi, McpApi, RegisterEphemeralRequest};
use simply_daemon::RemoteDaemon;

/// Key for storing the MCP server in serenity's TypeMap (to inject cache on ready).
pub struct McpServerKey;

impl TypeMapKey for McpServerKey {
    type Value = mcp::LuminaMcpServer;
}

/// Key for storing the daemon connection in serenity's TypeMap.
pub struct DaemonKey;

impl TypeMapKey for DaemonKey {
    type Value = Arc<dyn DaemonApi>;
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

    // Connect to simply-daemon
    let daemon_port = settings.daemon_port.unwrap_or(9800);
    let daemon_addr = format!("127.0.0.1:{daemon_port}");
    tracing::info!(addr = %daemon_addr, "connecting to simply-daemon");

    let daemon = RemoteDaemon::connect_as(&daemon_addr, "lumina").await?;
    tracing::info!("connected to simply-daemon");

    // Start Lumina's MCP server and register with daemon
    let http = Arc::new(serenity::http::Http::new(&token));
    let mcp_server = mcp::LuminaMcpServer::new(http);
    let _mcp_handle = simply_daemon::mcp::start_server(mcp_server.clone()).await?;
    let mcp_url = _mcp_handle.url();
    tracing::info!(url = %mcp_url, "lumina MCP server started");

    let tool_count = daemon
        .register_ephemeral_mcp(RegisterEphemeralRequest {
            id: "lumina-discord".to_string(),
            url: mcp_url,
        })
        .await?;
    tracing::info!(tool_count, "registered lumina MCP service with daemon");

    let daemon: Arc<dyn DaemonApi> = daemon.into_daemon();

    // Collect all auto-registered commands
    let registry = CommandRegistry::collect();
    tracing::info!(count = registry.len(), "commands registered");

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(token, intents)
        .event_handler(Handler {
            guild_ids: lumina_cfg.discord.guild_ids.iter().map(|&id| GuildId::new(id)).collect(),
            status_channel_id: lumina_cfg.discord.status_channel_id.map(ChannelId::new),
        })
        .type_map_insert::<DaemonKey>(daemon)
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
            _ => {}
        }
    }
}
