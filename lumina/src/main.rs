//! Lumina — Discord bot for the Simply platform.
//!
//! Connects to Discord via serenity and to simply-daemon via WebSocket.

mod commands;

use std::sync::Arc;

use commands::CommandRegistry;
use serenity::model::id::GuildId;
use serenity::prelude::*;
use simply_daemon::api::DaemonApi;
use simply_daemon::RemoteDaemon;

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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lumina=info".into()),
        )
        .init();

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

    let daemon = RemoteDaemon::connect(&daemon_addr).await?;
    let daemon: Arc<dyn DaemonApi> = daemon.into_daemon();
    tracing::info!("connected to simply-daemon");

    // Collect all auto-registered commands
    let registry = CommandRegistry::collect();
    tracing::info!(count = registry.len(), "commands registered");

    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(token, intents)
        .event_handler(Handler {
            guild_ids: lumina_cfg.discord.guild_ids.iter().map(|&id| GuildId::new(id)).collect(),
        })
        .type_map_insert::<DaemonKey>(daemon)
        .type_map_insert::<ConfigKey>(lumina_cfg)
        .type_map_insert::<CommandRegistry>(registry)
        .await?;

    tracing::info!("lumina starting");
    client.start().await?;

    Ok(())
}

struct Handler {
    guild_ids: Vec<GuildId>,
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: serenity::model::gateway::Ready) {
        tracing::info!(user = %ready.user.name, "connected to Discord");

        let data = ctx.data.read().await;
        let registry = data.get::<CommandRegistry>().expect("CommandRegistry missing");
        let definitions = registry.definitions();
        drop(data);

        for &guild_id in &self.guild_ids {
            if let Err(e) = guild_id.set_commands(&ctx.http, definitions.clone()).await {
                tracing::error!(guild_id = %guild_id, error = %e, "failed to register commands");
            } else {
                tracing::info!(guild_id = %guild_id, "registered slash commands");
            }
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: serenity::model::application::Interaction) {
        if let serenity::model::application::Interaction::Command(cmd) = interaction {
            let data = ctx.data.read().await;
            let registry = data.get::<CommandRegistry>().expect("CommandRegistry missing");
            registry.dispatch(&ctx, &cmd).await;
        }
    }
}
