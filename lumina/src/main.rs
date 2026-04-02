//! Lumina — Discord bot for the Simply platform.
//!
//! Connects to Discord via serenity and to simply-daemon via WebSocket.

use std::sync::Arc;

use serenity::prelude::*;
use simply_daemon::api::DaemonApi;
use simply_daemon::RemoteDaemon;

/// Key for storing the daemon connection in serenity's TypeMap.
struct DaemonKey;

impl TypeMapKey for DaemonKey {
    type Value = Arc<dyn DaemonApi>;
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

    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(token, intents)
        .event_handler(Handler)
        .type_map_insert::<DaemonKey>(daemon)
        .await?;

    tracing::info!("lumina starting");
    client.start().await?;

    Ok(())
}

struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: serenity::model::gateway::Ready) {
        tracing::info!(user = %ready.user.name, "connected to Discord");
    }
}
