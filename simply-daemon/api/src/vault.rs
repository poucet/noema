//! Markdown vault API.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use simply_rpc::RequestContext;
#[cfg(feature = "ts")]
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct VaultExportRequest {
    #[serde(default)]
    pub entity_ids: Vec<String>,
    #[serde(default)]
    pub include_frontmatter_identity: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct VaultExportSummary {
    pub exported_entities: usize,
    pub exported_files: usize,
    pub skipped_entities: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct VaultScanSummary {
    pub scanned_files: usize,
    pub actions: usize,
    pub projected_files: usize,
    pub conflicts: usize,
    pub missing_files: usize,
    pub unmanaged_files: usize,
    pub content_snapshots: usize,
    pub asset_projections: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct VaultConflictInfo {
    pub id: String,
    pub entity_id: Option<String>,
    pub path: String,
    pub reason: String,
    pub observed_entity_id: Option<String>,
    pub details: Option<Value>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct ResolveVaultConflictRequest {
    pub conflict_id: String,
    pub action: VaultConflictResolutionAction,
    pub entity_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub enum VaultConflictResolutionAction {
    Ignore,
    BindToEntity,
    AcceptNewPath,
    ForkAsNewDocument,
    RestoreOriginalId,
}

#[simply_rpc::rpc_service("vault")]
#[async_trait]
pub trait VaultApi: Send + Sync {
    /// Export document entities into the Markdown vault.
    #[rpc(post = "/vault/export", no_tool)]
    async fn export_documents(
        &self,
        ctx: &RequestContext,
        request: VaultExportRequest,
    ) -> anyhow::Result<VaultExportSummary>;

    /// Run a full vault reconciliation scan immediately.
    #[rpc(post = "/vault/scan", no_tool)]
    async fn scan(&self, ctx: &RequestContext) -> anyhow::Result<VaultScanSummary>;

    /// List unresolved vault conflicts.
    #[rpc(get = "/vault/conflicts", no_tool)]
    async fn list_conflicts(&self, ctx: &RequestContext) -> anyhow::Result<Vec<VaultConflictInfo>>;

    /// Resolve one vault conflict.
    #[rpc(post = "/vault/conflicts/resolve", no_tool)]
    async fn resolve_conflict(
        &self,
        ctx: &RequestContext,
        request: ResolveVaultConflictRequest,
    ) -> anyhow::Result<()>;
}
