use serde::Serialize;
use serde_json::Value;

use crate::RpcResult;

/// Wrap `Result<()>` — returns `true` on success.
pub fn call_unit(r: anyhow::Result<()>) -> RpcResult {
    r.map(|()| Value::Bool(true))
}

/// Wrap `Result<T: Serialize>` — serializes the value.
pub fn call_val<T: Serialize>(r: anyhow::Result<T>) -> RpcResult {
    r.map(|v| serde_json::to_value(v).unwrap_or_default())
}

/// Wrap an infallible `T: Serialize` (no Result wrapper).
pub fn call_raw<T: Serialize>(v: T) -> RpcResult {
    Ok(serde_json::to_value(v).unwrap_or_default())
}
