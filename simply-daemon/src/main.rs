//! Simply Daemon — standalone runner.
//!
//! Hosts the daemon as a separate process. Single port for REST, admin, and WebSocket.

use std::sync::Arc;

use simply_daemon::api::*;
use simply_daemon::storage::SqliteStores;
use simply_daemon::net;
use simply_rpc::RpcService;
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
    simply_daemon::oauth::providers::ensure_config();
    let mut settings = config::Settings::load();
    let port = settings.daemon_port.unwrap_or(config::DEFAULT_DAEMON_PORT);
    let daemon_secret = settings.ensure_daemon_secret().to_string();

    // Open storage and build daemon
    let stores = Arc::new(SqliteStores::open()?);
    let vector_store: Arc<dyn simply_core::embedding::VectorStore> = stores.sqlite();
    let token_store = Arc::new(simply_daemon::token_store::TransientTokenStore::new());
    let coordinator = Arc::new(simply_core::storage::coordinator::StorageCoordinator::from_stores(&*stores));

    let daemon = simply_daemon::builder::DaemonBuilder {
        stores: Arc::clone(&stores) as _,
        coordinator,
        vector_store,
        token_store: Arc::clone(&token_store),
        voice: simply_daemon::builder::create_voice_service(),
        skills: vec![],  // TODO: register GDocsSkill here
    }.build().await?;

    // Kill channel — CoreApi is special (not in daemon_tools, needs kill_tx from here)
    let (kill_tx, mut kill_rx) = mpsc::channel(1);
    let core_svc: Arc<dyn simply_rpc::RestService> =
        <dyn CoreApi>::service(Arc::new(simply_daemon::services::CoreService::new(kill_tx)));

    let rest_dispatcher = simply_daemon::builder::build_service_router(&daemon, vec![core_svc]);

    let tracker = net::server::ConnectionTracker::new();

    let oauth_tracker = daemon.oauth_tracker();
    let server = net::rest::start(net::rest::ServerConfig {
        rest_dispatcher,
        port,
        tracker,
        daemon_secret,
        user_store: stores.sqlite(),
        token_store,
        oauth_tracker: Some(oauth_tracker),
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
