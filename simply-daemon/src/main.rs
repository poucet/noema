//! Simply Daemon — standalone runner.
//!
//! Hosts the daemon as a separate process. Clients connect via WebSocket
//! (once implemented) or use the in-process library crate directly.

use std::sync::Arc;

use simply_daemon::api::SessionApi;
use simply_daemon::embedded::EmbeddedDaemon;
use simply_daemon::storage::SqliteStores;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "simply_daemon=info,simply_core=info".into()),
        )
        .init();

    tracing::info!("simply-daemon starting");

    // Load env file (API keys, etc.)
    config::load_env_file();

    // Open storage
    let stores = SqliteStores::open()?;
    let stores = Arc::new(stores);

    // Create daemon (starts MCP server, OAuth callback server, auto-connect)
    let daemon = EmbeddedDaemon::new(stores).await?;

    tracing::info!(
        oauth_callback = %daemon.oauth_redirect_uri(),
        "daemon ready"
    );

    // TODO: Start WebSocket server here (task 2.7)

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
