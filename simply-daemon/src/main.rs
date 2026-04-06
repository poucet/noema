//! Simply Daemon — standalone runner.
//!
//! Hosts the daemon as a separate process. Single port for REST, admin, and WebSocket.

use std::sync::Arc;

use simply_daemon::api::*;
use simply_daemon::embedded::EmbeddedDaemon;
use simply_daemon::storage::SqliteStores;
use simply_daemon::net;
use simply_rpc::{RestDispatcher, RpcService};
use simply_daemon::api::Daemon;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log to file if DAEMON_LOG_FILE is set, otherwise stderr
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "simply_daemon=info,simply_core=info".into());

    if let Ok(log_path) = std::env::var("DAEMON_LOG_FILE") {
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

    tracing::info!("simply-daemon starting");

    config::load_env_file();
    let settings = config::Settings::load();
    let port = settings.daemon_port.unwrap_or(config::DEFAULT_DAEMON_PORT);

    // Open storage and create daemon
    let stores = Arc::new(SqliteStores::open()?);
    let daemon = EmbeddedDaemon::new(stores).await?;

    // Kill channel — shared with CoreService so /daemon/kill actually works
    let (kill_tx, mut kill_rx) = mpsc::channel(1);
    let core_svc = Arc::new(simply_daemon::services::CoreService::new(kill_tx));

    // Register REST services from the daemon's inner components
    let session_svc: Arc<dyn SessionApi> = daemon.clone();
    let conversation_svc: Arc<dyn ConversationApi> = daemon.clone();

    let ws_dispatch = simply_daemon::ws_dispatch::build(Arc::clone(&session_svc));
    let tracker = net::server::ConnectionTracker::new();

    let rest_dispatcher = RestDispatcher::new()
        .register(<dyn SessionApi>::service(session_svc.clone()))
        .register(<dyn ConversationApi>::service(conversation_svc))
        .register(<dyn AssetApi>::service(daemon.asset_service()))
        .register(<dyn McpApi>::service(daemon.mcp_service()))
        .register(<dyn OAuthApi>::service(daemon.mcp_service()))
        .register(<dyn ModelApi>::service(daemon.model_service()))
        .register(<dyn VoiceApi>::service(daemon.voice_service()))
        .register(<dyn CoreApi>::service(core_svc));

    let server = net::rest::start(net::rest::ServerConfig {
        rest_dispatcher,
        ws_dispatch: Some(ws_dispatch),
        port,
        tracker,
    }).await?;

    let actual_port = server.port();
    tracing::info!(port = actual_port, "daemon ready");

    // Print port to stdout so callers (e.g. test harnesses) can discover it
    println!("{actual_port}");

    // Wait for shutdown signal (Ctrl+C, SIGTERM, or /kill REST endpoint)
    tokio::select! {
        _ = shutdown_signal() => {}
        _ = kill_rx.recv() => { tracing::info!("kill received via REST"); }
    }

    tracing::info!("shutting down");
    daemon.session().close_all_sessions().await?;
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
