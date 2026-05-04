//! Types for the Markdown vault projection.

use serde::{Deserialize, Serialize};

use crate::storage::ids::{EntityId, VaultConflictId};

/// Reconciliation status for a canonical vault file projection.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VaultSyncStatus(String);

impl VaultSyncStatus {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn synced() -> Self {
        Self::new("synced")
    }

    pub fn missing() -> Self {
        Self::new("missing")
    }

    pub fn conflict() -> Self {
        Self::new("conflict")
    }
}

impl From<&str> for VaultSyncStatus {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for VaultSyncStatus {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Reason a vault file could not be reconciled automatically.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VaultConflictReason(String);

impl VaultConflictReason {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn duplicate_id() -> Self {
        Self::new("duplicate_id")
    }

    pub fn changed_id() -> Self {
        Self::new("changed_id")
    }

    pub fn missing_id() -> Self {
        Self::new("missing_id")
    }

    pub fn unmanaged_file() -> Self {
        Self::new("unmanaged_file")
    }

    pub fn invalid_frontmatter() -> Self {
        Self::new("invalid_frontmatter")
    }

    pub fn unsupported_kind() -> Self {
        Self::new("unsupported_kind")
    }
}

impl From<&str> for VaultConflictReason {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for VaultConflictReason {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Canonical file projection for one markdown-backed entity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultFile {
    pub entity_id: EntityId,
    /// Vault-relative path using the scanner's normalized path format.
    pub path: String,
    pub file_key: Option<String>,
    pub mtime: Option<i64>,
    pub content_hash: String,
    pub frontmatter_hash: Option<String>,
    pub sync_status: VaultSyncStatus,
    pub last_seen_at: i64,
}

/// Persisted reconciliation conflict for UI/API resolution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultConflict {
    pub id: VaultConflictId,
    pub entity_id: Option<EntityId>,
    pub path: String,
    pub reason: VaultConflictReason,
    pub observed_entity_id: Option<EntityId>,
    pub details: Option<serde_json::Value>,
    pub created_at: i64,
}
