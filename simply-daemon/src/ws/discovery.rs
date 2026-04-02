//! Smart daemon discovery.
//!
//! - If a daemon is already running on the well-known port, connect to it.
//! - Otherwise, start an embedded daemon + WS server and become the host.
//!
//! The caller provides builders for the WS dispatch and REST dispatcher —
//! discovery doesn't know which services exist.

use std::sync::Arc;

use simply_rpc::Dispatcher;

use crate::api::DaemonApi;
use crate::embedded::EmbeddedDaemon;
use crate::remote::RemoteDaemon;
use crate::storage::SqliteStores;

use super::{rest, server};

const DEFAULT_DAEMON_PORT: u16 = 9800;

/// Result of daemon discovery.
pub enum DaemonHandle {
    /// This process is hosting the daemon. Dropping shuts down servers.
    Host {
        daemon: Arc<dyn DaemonApi>,
        _ws_server: server::ServerHandle,
        _rest_server: rest::RestHandle,
        /// Fires when `/kill` is called on the REST API.
        kill_rx: Option<tokio::sync::mpsc::Receiver<()>>,
    },
    /// Connected to a remote daemon.
    Remote {
        daemon: Arc<dyn DaemonApi>,
    },
}

impl DaemonHandle {
    pub fn daemon(&self) -> Arc<dyn DaemonApi> {
        match self {
            DaemonHandle::Host { daemon, .. } => Arc::clone(daemon),
            DaemonHandle::Remote { daemon, .. } => Arc::clone(daemon),
        }
    }

    pub fn is_host(&self) -> bool {
        matches!(self, DaemonHandle::Host { .. })
    }
}

/// Builders that the caller provides to wire up services.
pub struct ServiceBuilders {
    pub ws_dispatch: Box<dyn FnOnce(Arc<dyn DaemonApi>) -> server::DispatchFn + Send>,
    pub rest_dispatcher: Box<dyn FnOnce(Arc<dyn DaemonApi>) -> Dispatcher + Send>,
}

/// Try to connect to an existing daemon. If none is running, start one.
pub async fn connect_or_host(
    port: Option<u16>,
    builders: ServiceBuilders,
) -> anyhow::Result<DaemonHandle> {
    let port = port.unwrap_or(DEFAULT_DAEMON_PORT);
    let addr = format!("127.0.0.1:{}", port);

    // Try to connect with a short timeout
    let connect_result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        RemoteDaemon::connect(&addr),
    )
    .await;

    match connect_result {
        Ok(Ok(remote)) => {
            tracing::info!(port, "Connected to existing daemon");
            return Ok(DaemonHandle::Remote {
                daemon: remote.into_daemon(),
            });
        }
        Ok(Err(e)) => {
            tracing::debug!(port, error = %e, "No daemon at port, will start embedded");
        }
        Err(_) => {
            tracing::debug!(port, "Connect timed out, will start embedded");
        }
    }

    // No daemon running — start one
    tracing::info!(port, "Starting embedded daemon + WS server");

    config::load_env_file();
    let stores = Arc::new(SqliteStores::open()?);
    let daemon = EmbeddedDaemon::new(stores).await?;
    let daemon: Arc<dyn DaemonApi> = daemon;

    let ws_dispatch = (builders.ws_dispatch)(Arc::clone(&daemon));
    let ws_server = server::start(ws_dispatch, port).await?;

    let rest_port = port + 1;
    let rest_dispatcher = (builders.rest_dispatcher)(Arc::clone(&daemon));
    let (rest_server, kill_rx) = rest::start(rest_dispatcher, rest_port).await?;

    Ok(DaemonHandle::Host {
        daemon,
        _ws_server: ws_server,
        _rest_server: rest_server,
        kill_rx: Some(kill_rx),
    })
}
