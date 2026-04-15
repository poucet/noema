//! Networking layer — HTTP/REST server, WebSocket server, client transport, and daemon discovery.

pub mod admin_api;
pub mod auth_routes;
pub mod mcp_auth;
pub mod protocol;
pub mod server;
pub mod rest;
pub(crate) mod client;
pub mod discovery;

pub use simply_rpc::ws_client::ConnectionState;
pub use discovery::{connect_or_host, DaemonHandle};
pub use rest::ServerHandle;
