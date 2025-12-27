//! Google Docs MCP Server for Noema
//!
//! This crate provides MCP tools for:
//! - Listing Google Docs from Drive
//! - Getting document content
//! - Extracting documents with tabs and images
//!
//! Can be used as:
//! - An embedded server (via `start_server`)
//! - A standalone binary (`noema-mcp-gdocs`)

pub mod google_api;
pub mod tools;
mod well_known;

pub use google_api::{ExtractedDocument, ExtractedImage, ExtractedTab, GoogleDocsClient};
pub use tools::GoogleDocsServer;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower_service::Service;
use tracing::info;

type BoxBody = http_body_util::combinators::BoxBody<Bytes, Infallible>;

/// Handle the .well-known/oauth-authorization-server endpoint
fn handle_well_known() -> Result<Response<BoxBody>, Infallible> {
    let metadata = well_known::google_oauth_metadata();
    let json = serde_json::to_string(&metadata).unwrap_or_default();

    let body = Full::new(Bytes::from(json)).map_err(|_| -> Infallible { unreachable!() }).boxed();

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap();

    Ok(response)
}

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

/// Start the MCP server on a random port
///
/// Returns a handle that can be used to get the port and stop the server.
pub async fn start_server() -> anyhow::Result<ServerHandle> {
    start_server_on("127.0.0.1", 0).await
}

/// Start the MCP server on the specified host and port
///
/// Use port 0 to get a random available port.
pub async fn start_server_on(host: &str, port: u16) -> anyhow::Result<ServerHandle> {
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    let actual_port = local_addr.port();

    info!("Starting Google Docs MCP server on {}", local_addr);

    let config = StreamableHttpServerConfig::default();
    let session_manager = Arc::new(LocalSessionManager::default());

    let mcp_service = StreamableHttpService::new(
        || Ok(GoogleDocsServer::new()),
        session_manager,
        config,
    );

    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

    // Spawn the server task
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    info!("Shutting down Google Docs MCP server");
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
                                        hyper::service::service_fn(move |req: Request<hyper::body::Incoming>| {
                                            let mut svc = service.clone();
                                            async move {
                                                // Handle .well-known OAuth discovery endpoint
                                                if req.uri().path() == "/.well-known/oauth-authorization-server" {
                                                    return handle_well_known();
                                                }
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
