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

/// Encode `Vec<u8>` as base64 string for transport.
pub fn encode_base64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Decode base64 string back to `Vec<u8>`.
pub fn decode_base64(s: &str) -> anyhow::Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| anyhow::anyhow!("invalid base64: {e}"))
}

/// Serde module for `Vec<u8>` ↔ base64 string.
///
/// Use with `#[serde(with = "simply_rpc::base64_bytes")]` on `Vec<u8>` fields
/// so they serialize as base64 strings in JSON instead of number arrays.
pub mod base64_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(data: &Vec<u8>, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&base64::engine::general_purpose::STANDARD.encode(data))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(de)?;
        base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(serde::de::Error::custom)
    }
}
