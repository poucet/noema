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
pub mod protocol;
mod response;
mod service;
mod stream;
pub mod ws_client;
pub mod ws_server;

pub use client::RpcConnection;
pub use context::DispatchResult;
pub use stream::StreamHandle;
pub use helpers::{base64_bytes, call_raw, call_unit, call_val, decode_base64, encode_base64};
pub use meta::{check_compat, HttpMethod, MethodMeta, RouteKind, RouteMeta, ServiceMeta, ServiceMetaWire};
pub use response::{BinaryResponse, BinaryUpload};
pub use service::{ServiceRouter, RestResult, RestService, RpcService, WsDispatchResult};

/// Result type for RPC dispatch — `Ok(Value)` or error.
pub type RpcResult = anyhow::Result<serde_json::Value>;

// Re-export the proc macro
pub use simply_rpc_macros::rpc_service;

// Re-export schemars so downstream crates don't need it as a direct dependency
pub use schemars;
