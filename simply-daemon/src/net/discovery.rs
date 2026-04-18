//! Smart daemon discovery.
//!
//! - If a daemon is already running on the well-known port, connect to it.
//! - Otherwise, start an embedded daemon and become the host.

use std::sync::Arc;

use tokio::sync::watch;

use simply_rpc::{ServiceRouter, RpcService};

use crate::api::*;
use crate::embedded::EmbeddedDaemon;
use crate::remote::RemoteDaemon;
use crate::services::CoreService;
use crate::storage::SqliteStores;

use simply_rpc::ws_client::ConnectionState;
use super::{rest, server};

use config::DEFAULT_DAEMON_PORT;

/// Result of daemon discovery.
pub enum DaemonHandle {
    /// This process is hosting the daemon. Dropping shuts down servers.
    Host {
        daemon: Arc<dyn Daemon>,
        _server: rest::ServerHandle,
        kill_rx: tokio::sync::mpsc::Receiver<()>,
    },
    /// Connected to a remote daemon. Reconnects automatically on disconnect.
    Remote {
        daemon: Arc<dyn Daemon>,
        remote: Arc<RemoteDaemon>,
    },
}

impl DaemonHandle {
    pub fn daemon(&self) -> Arc<dyn Daemon> {
        match self {
            DaemonHandle::Host { daemon, .. } => Arc::clone(daemon),
            DaemonHandle::Remote { daemon, .. } => Arc::clone(daemon),
        }
    }

    pub fn is_host(&self) -> bool {
        matches!(self, DaemonHandle::Host { .. })
    }

    /// Wait for a kill signal. Only fires for hosted daemons (via `/daemon/kill`).
    /// For remote daemons, this future never resolves.
    pub async fn wait_for_kill(&mut self) {
        match self {
            DaemonHandle::Host { kill_rx, .. } => { kill_rx.recv().await; }
            DaemonHandle::Remote { .. } => std::future::pending().await,
        }
    }

    /// Current connection state. Host is always connected.
    pub fn connection_state(&self) -> ConnectionState {
        match self {
            DaemonHandle::Host { .. } => ConnectionState::Connected,
            DaemonHandle::Remote { remote, .. } => remote.connection_state(),
        }
    }

    /// Watch connection state changes. Host never changes.
    pub fn watch_connection_state(&self) -> watch::Receiver<ConnectionState> {
        match self {
            DaemonHandle::Host { .. } => {
                let (_, rx) = watch::channel(ConnectionState::Connected);
                rx
            }
            DaemonHandle::Remote { remote, .. } => remote.watch_connection_state(),
        }
    }
}

/// Try to connect to an existing daemon. If none is running, start one.
///
/// When hosting, starts a full server with REST + WS (session streaming)
/// and a kill channel for `/daemon/kill`.
pub async fn connect_or_host(
    port: Option<u16>,
    client_name: &str,
    user_id: Option<&str>,
) -> anyhow::Result<DaemonHandle> {
    let port = port.unwrap_or(DEFAULT_DAEMON_PORT);
    let addr = format!("127.0.0.1:{}", port);

    let mut settings = config::Settings::load();
    let daemon_secret = settings.ensure_daemon_secret().to_string();

    // Try to connect with a short timeout
    let connect_result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        RemoteDaemon::connect_as(&addr, client_name, &daemon_secret, user_id),
    )
    .await;

    match connect_result {
        Ok(Ok(remote)) => {
            tracing::info!(port, "Connected to existing daemon");
            let daemon = Arc::clone(&remote) as Arc<dyn Daemon>;
            return Ok(DaemonHandle::Remote { daemon, remote });
        }
        Ok(Err(e)) => {
            tracing::debug!(port, error = %e, "No daemon at port, will start embedded");
        }
        Err(_) => {
            tracing::debug!(port, "Connect timed out, will start embedded");
        }
    }

    // No daemon running — start one with full REST + WS server
    tracing::info!(port, "Starting embedded daemon");

    config::load_env_file();
    let stores = Arc::new(SqliteStores::open()?);
    let user_store = stores.sqlite();
    let vector_store: Arc<dyn simply_core::embedding::VectorStore> = stores.sqlite();
    let token_store = Arc::new(crate::token_store::TransientTokenStore::new());
    let coordinator = Arc::new(simply_core::storage::coordinator::StorageCoordinator::from_stores(&*stores));
    let daemon = crate::builder::DaemonBuilder {
        stores: Arc::clone(&stores) as _,
        coordinator,
        vector_store,
        token_store: Arc::clone(&token_store),
        voice: crate::builder::create_voice_service(),
        skills: vec![],
    }.build().await?;

    let (kill_tx, kill_rx) = tokio::sync::mpsc::channel(1);
    let core_svc: Arc<dyn simply_rpc::RestService> =
        <dyn CoreApi>::service(Arc::new(CoreService::new(kill_tx)));
    let rest_dispatcher = crate::builder::build_service_router(&daemon, vec![core_svc]);

    let tracker = server::ConnectionTracker::new();
    let oauth_tracker = daemon.oauth_tracker();
    let server = rest::start(rest::ServerConfig {
        rest_dispatcher,
        port,
        tracker,
        daemon_secret,
        user_store,
        token_store,
        oauth_tracker: Some(oauth_tracker),
    }).await?;

    Ok(DaemonHandle::Host {
        daemon,
        _server: server,
        kill_rx,
    })
}
