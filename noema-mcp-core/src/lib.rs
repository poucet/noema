//! Noema Core MCP Server
//!
//! Exposes noema's internal capabilities as standard MCP tools:
//! - `spawn_agent` - spawn subconversations for complex subtasks
//!
//! This server is stateless - agents enrich tool calls with context
//! (conversation_id, turn_id, etc) before forwarding to this server.

pub mod tools;

pub use tools::NoemaCoreServer;

use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower_service::Service;
use tracing::info;

/// Handle to a running server that can be used to stop it
pub struct ServerHandle {
    shutdown_tx: oneshot::Sender<()>,
    port: u16,
}

impl ServerHandle {
    /// Get the port the server is running on
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get the URL for the MCP endpoint
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.port)
    }

    /// Stop the server
    pub fn stop(self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// Start the noema-core MCP server on a random port
pub async fn start_server(server: NoemaCoreServer) -> anyhow::Result<ServerHandle> {
    start_server_on("127.0.0.1", 0, server).await
}

/// Start the noema-core MCP server on the specified host and port
pub async fn start_server_on(
    host: &str,
    port: u16,
    server: NoemaCoreServer,
) -> anyhow::Result<ServerHandle> {
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    let listener = TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    let actual_port = local_addr.port();

    info!("Starting Noema Core MCP server on {}", local_addr);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        let config = StreamableHttpServerConfig::default();
        let session_manager = Arc::new(LocalSessionManager::default());

        let mcp_service = StreamableHttpService::new(
            move || Ok(server.clone()),
            session_manager,
            config,
        );

        let mut shutdown_rx = shutdown_rx;

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    info!("Shutting down Noema Core MCP server");
                    break;
                }
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let io = TokioIo::new(stream);
                            let service = mcp_service.clone();

                            tokio::spawn(async move {
                                if let Err(err) = http1::Builder::new()
                                    .serve_connection(
                                        io,
                                        hyper::service::service_fn(move |req| {
                                            let mut svc = service.clone();
                                            async move {
                                                svc.call(req).await
                                            }
                                        }),
                                    )
                                    .await
                                {
                                    tracing::error!("Error serving connection: {:?}", err);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Failed to accept connection: {}", e);
                        }
                    }
                }
            }
        }
    });

    Ok(ServerHandle {
        shutdown_tx,
        port: actual_port,
    })
}
