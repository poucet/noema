//! `simply-rpc` — Generic trait-over-network RPC framework.
//!
//! Annotate a Rust trait with `#[rpc_service("prefix")]` to auto-generate:
//! - A server dispatch struct implementing [`RpcService`]
//! - A client impl macro for any type implementing [`RpcClient`]
//!
//! The framework is transport-agnostic — it handles `(method, params) -> Result<Value>`
//! dispatch. WebSocket, REST, or any other transport can be layered on top.

mod client;
mod context;
mod helpers;
pub mod meta;
mod service;

pub use client::RpcClient;
pub use context::DispatchResult;
pub use helpers::{call_raw, call_unit, call_val};
pub use meta::{check_compat, MethodMeta, ServiceMeta, ServiceMetaWire};
pub use service::{Dispatcher, RpcService};

/// Result type for RPC dispatch — `Ok(Value)` or error.
pub type RpcResult = anyhow::Result<serde_json::Value>;

// Re-export the proc macro
pub use simply_rpc_macros::rpc_service;
