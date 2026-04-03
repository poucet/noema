//! Daemon-level operations: health, version, shutdown.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonHealth {
    pub status: String,
}

#[simply_rpc::rpc_service("daemon")]
#[async_trait]
pub trait DaemonInfoApi: Send + Sync {
    /// Check daemon health.
    #[rpc(get = "/daemon")]
    async fn health(&self) -> anyhow::Result<DaemonHealth>;

    /// Shut down the daemon.
    #[rpc(post = "/daemon/kill", no_tool)]
    async fn kill(&self) -> anyhow::Result<()>;

    /// Get daemon version.
    #[rpc(get = "/daemon/version")]
    async fn version(&self) -> anyhow::Result<String>;
}
