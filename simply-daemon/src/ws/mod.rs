//! WebSocket server, client transport, and daemon discovery.

pub mod protocol;
pub mod server;
pub(crate) mod client;
pub mod discovery;

pub use discovery::{connect_or_host, DaemonHandle};
pub use server::ServerHandle;
