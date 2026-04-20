//! Skills API — read-only listing of registered skills (embedded + remote WS).
//!
//! Skill registration itself does NOT go through this API — it happens via
//! `Daemon::register_skill` (in-process or WS reverse-RPC) because skills
//! need a callback channel for tool calls. This API is just for querying.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
#[cfg(feature = "ts")]
use ts_rs::TS;

/// A skill registered with the daemon — in-process or via WS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "../admin/src/lib/generated/types/"))]
pub struct SkillInfo {
    /// Provider ID (e.g. "gdocs", or a WS connection ID).
    pub id: String,
    pub display_name: String,
    /// "embedded" (in-process) or "ws" (remote client).
    pub kind: String,
    pub tool_count: usize,
    pub is_connected: bool,
    /// OAuth provider IDs this skill depends on, if any.
    pub oauth_provider_ids: Vec<String>,
}

#[simply_rpc::rpc_service("skills")]
#[async_trait]
pub trait SkillsApi: Send + Sync {
    /// List all skills registered with the daemon.
    #[rpc(get = "/skills", no_tool)]
    async fn list_skills(&self) -> anyhow::Result<Vec<SkillInfo>>;
}
