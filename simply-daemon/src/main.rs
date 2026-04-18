//! Simply Daemon — standalone runner.

use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let stores = Arc::new(simply_daemon::storage::SqliteStores::open()?);
    let vector_store: Arc<dyn simply_core::embedding::VectorStore> = stores.sqlite();
    let token_store = Arc::new(simply_daemon::services::token_store::TransientTokenStore::new());
    let coordinator = Arc::new(simply_core::storage::coordinator::StorageCoordinator::from_stores(&*stores));

    let handle = simply_daemon::builder::DaemonBuilder {
        stores: Arc::clone(&stores) as _,
        coordinator,
        vector_store,
        token_store,
        voice: simply_daemon::builder::create_voice_service(),
        skill_factories: vec![
            Box::new(|daemon| Box::new(mcp_gdocs::GDocsSkill::new(daemon))),
        ],
    }.build().await?;

    handle.serve(port, daemon_secret).await
}
