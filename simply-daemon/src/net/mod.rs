//! Networking layer — HTTP/REST server, WebSocket server, client transport, and daemon discovery.

pub mod protocol;
pub mod server;
pub mod rest;
pub(crate) mod client;
pub mod discovery;

pub use simply_rpc::ws_client::ConnectionState;
pub use discovery::{connect_or_host, DaemonHandle, ServiceBuilders};
pub use rest::ServerHandle;
