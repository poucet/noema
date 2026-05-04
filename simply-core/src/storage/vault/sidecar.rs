//! Portable vault sidecar manifest.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::storage::ids::EntityId;
use crate::storage::types::{EntityType, RelationType, VaultFile, VaultSyncStatus};

pub const SIDECAR_DIR: &str = ".noema";
pub const SIDECAR_FILE: &str = "vault-index.json";
pub const SIDECAR_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSidecarManifest {
    pub version: u32,
    #[serde(default)]
    pub files: BTreeMap<String, VaultSidecarFile>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub ignored_paths: BTreeSet<String>,
}

impl Default for VaultSidecarManifest {
    fn default() -> Self {
        Self {
            version: SIDECAR_VERSION,
            files: BTreeMap::new(),
            ignored_paths: BTreeSet::new(),
        }
    }
}

impl VaultSidecarManifest {
    pub fn from_vault_files(files: &[VaultFile]) -> Self {
        let mut manifest = Self::default();
        for file in files {
            manifest
                .files
                .insert(file.path.clone(), VaultSidecarFile::from_vault_file(file));
        }
        manifest
    }

    pub fn preserve_user_fields_from(&mut self, existing: &Self) {
        self.ignored_paths = existing.ignored_paths.clone();
        for (path, existing_file) in &existing.files {
            if let Some(file) = self.files.get_mut(path) {
                file.preserve_metadata_from(existing_file);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSidecarFile {
    pub entity_id: EntityId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<EntityType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_entity_id: Option<EntityId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_relation: Option<RelationType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime: Option<i64>,
    pub content_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontmatter_hash: Option<String>,
    pub sync_status: VaultSyncStatus,
}

impl VaultSidecarFile {
    pub fn from_vault_file(file: &VaultFile) -> Self {
        Self {
            entity_id: file.entity_id.clone(),
            kind: None,
            title: None,
            origin: None,
            parent_entity_id: None,
            parent_relation: None,
            position: None,
            file_key: file.file_key.clone(),
            mtime: file.mtime,
            content_hash: file.content_hash.clone(),
            frontmatter_hash: file.frontmatter_hash.clone(),
            sync_status: file.sync_status.clone(),
        }
    }

    pub fn preserve_metadata_from(&mut self, existing: &Self) {
        self.kind = existing.kind.clone();
        self.title = existing.title.clone();
        self.origin = existing.origin.clone();
        self.parent_entity_id = existing.parent_entity_id.clone();
        self.parent_relation = existing.parent_relation.clone();
        self.position = existing.position;
    }
}

pub fn sidecar_path(root: &Path) -> PathBuf {
    root.join(SIDECAR_DIR).join(SIDECAR_FILE)
}

pub fn read_sidecar_manifest(root: &Path) -> Result<Option<VaultSidecarManifest>> {
    let path = sidecar_path(root);
    if !path.exists() {
        return Ok(None);
    }

    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read vault sidecar {}", path.display()))?;
    let manifest = serde_json::from_str(&text)
        .with_context(|| format!("Failed to parse vault sidecar {}", path.display()))?;
    Ok(Some(manifest))
}

pub fn write_sidecar_manifest(root: &Path, manifest: &VaultSidecarManifest) -> Result<()> {
    let dir = root.join(SIDECAR_DIR);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create vault sidecar directory {}", dir.display()))?;

    let path = dir.join(SIDECAR_FILE);
    let temp_path = dir.join(format!("{SIDECAR_FILE}.tmp"));
    let text =
        serde_json::to_string_pretty(manifest).context("Failed to serialize vault sidecar")?;

    std::fs::write(&temp_path, format!("{text}\n")).with_context(|| {
        format!(
            "Failed to write vault sidecar temp file {}",
            temp_path.display()
        )
    })?;
    std::fs::rename(&temp_path, &path).with_context(|| {
        format!(
            "Failed to replace vault sidecar {} with {}",
            path.display(),
            temp_path.display()
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_file(entity_id: &str, path: &str) -> VaultFile {
        VaultFile {
            entity_id: EntityId::from_string(entity_id),
            path: path.to_string(),
            file_key: None,
            mtime: None,
            content_hash: format!("hash-{path}"),
            frontmatter_hash: None,
            sync_status: VaultSyncStatus::synced(),
            last_seen_at: 0,
        }
    }

    #[test]
    fn builds_manifest_from_projection_rows() {
        let manifest = VaultSidecarManifest::from_vault_files(&[
            vault_file("ent_b", "b.md"),
            vault_file("ent_a", "a.md"),
        ]);

        assert_eq!(manifest.version, SIDECAR_VERSION);
        assert_eq!(
            manifest
                .files
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["a.md", "b.md"]
        );
        assert_eq!(manifest.files["a.md"].entity_id.as_str(), "ent_a");
    }
}
