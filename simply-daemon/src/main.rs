//! Simply Daemon — standalone runner.
//!
//! Hosts the daemon as a separate process. Clients connect via WebSocket.

use std::sync::Arc;

use simply_daemon::api::{DaemonApi, SessionApi};
use simply_daemon::embedded::EmbeddedDaemon;
use simply_daemon::storage::SqliteStores;
use simply_daemon::ws;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "simply_daemon=info,simply_core=info".into()),
        )
        .init();

    tracing::info!("simply-daemon starting");

    config::load_env_file();
    let settings = config::Settings::load();
    let port = settings.daemon_port.unwrap_or(9800);

    // Open storage and create daemon
    let stores = Arc::new(SqliteStores::open()?);
    let daemon = EmbeddedDaemon::new(stores).await?;
    let daemon: Arc<dyn DaemonApi> = daemon;

    // Start WebSocket server
    let _ws_server = ws::server::start(Arc::clone(&daemon), port).await?;

    tracing::info!(
        ws_port = port,
        "daemon ready"
    );

    // Wait for shutdown signal
    shutdown_signal().await;

    tracing::info!("shutting down");
    daemon.close_all_sessions().await?;
    tracing::info!("shutdown complete");

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
}
