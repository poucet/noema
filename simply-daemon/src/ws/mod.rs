//! WebSocket server, REST server, client transport, and daemon discovery.

pub mod protocol;
pub mod server;
pub mod rest;
pub(crate) mod client;
pub mod discovery;

pub use discovery::{connect_or_host, DaemonHandle, ServiceBuilders};
pub use server::ServerHandle;
pub use rest::RestHandle;
