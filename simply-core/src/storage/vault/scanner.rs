//! Read-only Markdown vault scanning and reconciliation planning.

use anyhow::{Context, Result};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::storage::helper::content_hash;
use crate::storage::ids::EntityId;
use crate::storage::types::{EntityType, VaultConflictReason, VaultFile};
use crate::storage::vault::markdown::{parse_markdown, split_markdown};
use crate::storage::vault::sidecar::SIDECAR_DIR;

/// File observed during a vault scan.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservedVaultFile {
    pub path: String,
    pub mtime: Option<i64>,
    pub content_hash: String,
    pub frontmatter_hash: Option<String>,
    pub identity: ObservedVaultIdentity,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObservedVaultIdentity {
    Managed {
        entity_id: EntityId,
        kind: EntityType,
    },
    Unidentified {
        kind: Option<EntityType>,
    },
    InvalidFrontmatter {
        error: String,
    },
    UnsupportedKind {
        entity_id: Option<EntityId>,
        kind: Option<EntityType>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct VaultReconciliationPlan {
    pub actions: Vec<VaultReconciliationAction>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VaultReconciliationAction {
    KeepSynced {
        entity_id: EntityId,
        path: String,
    },
    UpdatePath {
        entity_id: EntityId,
        from_path: String,
        to_path: String,
    },
    MarkMissing {
        entity_id: EntityId,
        path: String,
    },
    ImportUnknown {
        entity_id: EntityId,
        kind: EntityType,
        path: String,
    },
    Conflict {
        path: String,
        reason: VaultConflictReason,
        entity_id: Option<EntityId>,
        observed_entity_id: Option<EntityId>,
        details: serde_json::Value,
    },
    Unmanaged {
        path: String,
        reason: VaultConflictReason,
        details: serde_json::Value,
    },
}

/// Defines whether Noema derives managed identity from Markdown frontmatter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultIdentityMode {
    /// Identity comes from the vault projection/sidecar state. Frontmatter is
    /// user-owned metadata and is not interpreted as Noema identity.
    Projection,
    /// Identity comes from Noema-owned frontmatter fields when present.
    Frontmatter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultScanOptions {
    pub identity_mode: VaultIdentityMode,
    pub supported_kind_prefixes: Vec<String>,
}

impl Default for VaultScanOptions {
    fn default() -> Self {
        Self {
            identity_mode: VaultIdentityMode::Projection,
            supported_kind_prefixes: vec!["document::".to_string()],
        }
    }
}

/// Recursively scan a vault root for Markdown files. Does not mutate storage.
pub fn scan_vault(root: &Path, options: &VaultScanOptions) -> Result<Vec<ObservedVaultFile>> {
    let mut files = Vec::new();
    scan_dir(root, root, options, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Scan a set of vault-relative or absolute paths. Missing paths are ignored so
/// the reconciliation planner can mark projected files missing.
pub fn scan_vault_paths(
    root: &Path,
    paths: &[PathBuf],
    options: &VaultScanOptions,
) -> Result<Vec<ObservedVaultFile>> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();

    for path in paths {
        let path = if path.is_absolute() {
            path.clone()
        } else {
            root.join(path)
        };

        if !path.exists() {
            continue;
        }

        if path.is_dir() {
            scan_dir(root, &path, options, &mut files)?;
        } else if path.is_file() && is_markdown_file(&path) {
            let file = scan_file(root, &path, options)?;
            if seen.insert(file.path.clone()) {
                files.push(file);
            }
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Build a reconciliation plan from projected known files and observed files.
pub fn plan_reconciliation(
    known_files: &[VaultFile],
    observed_files: &[ObservedVaultFile],
) -> VaultReconciliationPlan {
    plan_reconciliation_with_options(known_files, observed_files, &VaultScanOptions::default())
}

/// Build a reconciliation plan with explicit vault identity policy.
pub fn plan_reconciliation_with_options(
    known_files: &[VaultFile],
    observed_files: &[ObservedVaultFile],
    options: &VaultScanOptions,
) -> VaultReconciliationPlan {
    plan_reconciliation_inner(known_files, observed_files, MissingScope::All, options)
}

/// Build a scoped reconciliation plan. Only known files whose path appears in
/// `missing_paths` can be marked missing, but all known files are still used for
/// ID/path conflict detection.
pub fn plan_scoped_reconciliation(
    known_files: &[VaultFile],
    observed_files: &[ObservedVaultFile],
    missing_paths: &HashSet<String>,
) -> VaultReconciliationPlan {
    plan_scoped_reconciliation_with_options(
        known_files,
        observed_files,
        missing_paths,
        &VaultScanOptions::default(),
    )
}

/// Build a scoped reconciliation plan with explicit vault identity policy.
pub fn plan_scoped_reconciliation_with_options(
    known_files: &[VaultFile],
    observed_files: &[ObservedVaultFile],
    missing_paths: &HashSet<String>,
    options: &VaultScanOptions,
) -> VaultReconciliationPlan {
    plan_reconciliation_inner(
        known_files,
        observed_files,
        MissingScope::Paths(missing_paths),
        options,
    )
}

fn plan_reconciliation_inner(
    known_files: &[VaultFile],
    observed_files: &[ObservedVaultFile],
    missing_scope: MissingScope<'_>,
    options: &VaultScanOptions,
) -> VaultReconciliationPlan {
    let known_by_entity: HashMap<_, _> = known_files
        .iter()
        .map(|file| (file.entity_id.as_str().to_string(), file))
        .collect();
    let known_by_path: HashMap<_, _> = known_files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let observed_paths: HashSet<_> = observed_files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let missing_known_by_hash =
        missing_known_by_content_hash(known_files, &observed_paths, &missing_scope);

    let mut actions = Vec::new();
    let mut covered_known_entities = HashSet::new();
    let mut managed_by_entity: HashMap<String, Vec<&ObservedVaultFile>> = HashMap::new();

    for file in observed_files {
        match &file.identity {
            ObservedVaultIdentity::Managed { entity_id, .. } => {
                managed_by_entity
                    .entry(entity_id.as_str().to_string())
                    .or_default()
                    .push(file);
            }
            ObservedVaultIdentity::Unidentified { kind } => {
                classify_unidentified_file(
                    file,
                    kind.as_ref().map(EntityType::as_str),
                    options.identity_mode,
                    &known_by_path,
                    &missing_known_by_hash,
                    &mut covered_known_entities,
                    &mut actions,
                );
            }
            ObservedVaultIdentity::InvalidFrontmatter { error } => {
                classify_invalid_file(
                    file,
                    VaultConflictReason::invalid_frontmatter(),
                    json!({ "error": error }),
                    &known_by_path,
                    &mut covered_known_entities,
                    &mut actions,
                );
            }
            ObservedVaultIdentity::UnsupportedKind { entity_id, kind } => {
                classify_invalid_file(
                    file,
                    VaultConflictReason::unsupported_kind(),
                    json!({
                        "kind": kind.as_ref().map(EntityType::as_str),
                    }),
                    &known_by_path,
                    &mut covered_known_entities,
                    &mut actions,
                );
                if let Some(entity_id) = entity_id {
                    covered_known_entities.insert(entity_id.as_str().to_string());
                }
            }
        }
    }

    for (entity_id, files) in managed_by_entity {
        classify_managed_files(
            &entity_id,
            files,
            &known_by_entity,
            &known_by_path,
            &mut covered_known_entities,
            &mut actions,
        );
    }

    for known in known_files {
        if covered_known_entities.contains(known.entity_id.as_str()) {
            continue;
        }
        if observed_paths.contains(known.path.as_str()) || !missing_scope.contains(&known.path) {
            continue;
        }
        actions.push(VaultReconciliationAction::MarkMissing {
            entity_id: known.entity_id.clone(),
            path: known.path.clone(),
        });
    }

    VaultReconciliationPlan { actions }
}

enum MissingScope<'a> {
    All,
    Paths(&'a HashSet<String>),
}

impl MissingScope<'_> {
    fn contains(&self, path: &str) -> bool {
        match self {
            Self::All => true,
            Self::Paths(paths) => paths.iter().any(|scope| {
                path == scope
                    || path
                        .strip_prefix(scope)
                        .is_some_and(|rest| rest.starts_with('/'))
            }),
        }
    }
}

fn missing_known_by_content_hash<'a>(
    known_files: &'a [VaultFile],
    observed_paths: &HashSet<&str>,
    missing_scope: &MissingScope<'_>,
) -> HashMap<&'a str, Vec<&'a VaultFile>> {
    let mut by_hash: HashMap<&str, Vec<&VaultFile>> = HashMap::new();
    for known in known_files {
        if observed_paths.contains(known.path.as_str()) || !missing_scope.contains(&known.path) {
            continue;
        }
        by_hash
            .entry(known.content_hash.as_str())
            .or_default()
            .push(known);
    }
    by_hash
}

fn scan_dir(
    root: &Path,
    dir: &Path,
    options: &VaultScanOptions,
    files: &mut Vec<ObservedVaultFile>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read vault directory {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if entry.file_name() == std::ffi::OsStr::new(SIDECAR_DIR) {
                continue;
            }
            scan_dir(root, &path, options, files)?;
        } else if file_type.is_file() && is_markdown_file(&path) {
            files.push(scan_file(root, &path, options)?);
        }
    }
    Ok(())
}

fn scan_file(root: &Path, path: &Path, options: &VaultScanOptions) -> Result<ObservedVaultFile> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read vault file {}", path.display()))?;
    let split = split_markdown(&text);

    let relative_path = normalize_relative_path(
        path.strip_prefix(root)
            .with_context(|| format!("Failed to normalize vault path {}", path.display()))?,
    );
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("Failed to stat vault file {}", path.display()))?;

    let frontmatter_hash = split.raw_frontmatter.map(content_hash);
    let content_hash = content_hash(split.body);
    let mtime = metadata.modified().ok().and_then(modified_millis);

    let identity = match options.identity_mode {
        VaultIdentityMode::Projection => ObservedVaultIdentity::Unidentified { kind: None },
        VaultIdentityMode::Frontmatter => match parse_markdown(&text) {
            Ok(document) => classify_frontmatter(document.frontmatter, options),
            Err(e) => ObservedVaultIdentity::InvalidFrontmatter {
                error: e.to_string(),
            },
        },
    };

    Ok(ObservedVaultFile {
        path: relative_path,
        mtime,
        content_hash,
        frontmatter_hash,
        identity,
    })
}

fn classify_frontmatter(
    frontmatter: Option<crate::storage::vault::markdown::Frontmatter>,
    options: &VaultScanOptions,
) -> ObservedVaultIdentity {
    let Some(frontmatter) = frontmatter else {
        return ObservedVaultIdentity::Unidentified { kind: None };
    };
    let kind = frontmatter.kind;
    let entity_id = frontmatter.id;

    if !kind
        .as_ref()
        .map(|kind| is_supported_kind(kind, options))
        .unwrap_or(true)
    {
        return ObservedVaultIdentity::UnsupportedKind { entity_id, kind };
    }

    match entity_id {
        Some(entity_id) => ObservedVaultIdentity::Managed {
            entity_id,
            kind: kind.unwrap_or_else(|| EntityType::new("document::note")),
        },
        None => ObservedVaultIdentity::Unidentified { kind },
    }
}

fn classify_unidentified_file(
    file: &ObservedVaultFile,
    kind: Option<&str>,
    identity_mode: VaultIdentityMode,
    known_by_path: &HashMap<&str, &VaultFile>,
    missing_known_by_content_hash: &HashMap<&str, Vec<&VaultFile>>,
    covered_known_entities: &mut HashSet<String>,
    actions: &mut Vec<VaultReconciliationAction>,
) {
    if let Some(known) = known_by_path.get(file.path.as_str()) {
        covered_known_entities.insert(known.entity_id.as_str().to_string());
        match identity_mode {
            VaultIdentityMode::Projection => actions.push(VaultReconciliationAction::KeepSynced {
                entity_id: known.entity_id.clone(),
                path: file.path.clone(),
            }),
            VaultIdentityMode::Frontmatter => actions.push(VaultReconciliationAction::Conflict {
                path: file.path.clone(),
                reason: VaultConflictReason::missing_id(),
                entity_id: Some(known.entity_id.clone()),
                observed_entity_id: None,
                details: json!({ "kind": kind }),
            }),
        }
        return;
    }

    if identity_mode == VaultIdentityMode::Projection {
        if let Some(candidates) = missing_known_by_content_hash.get(file.content_hash.as_str()) {
            if let [known] = candidates.as_slice() {
                covered_known_entities.insert(known.entity_id.as_str().to_string());
                actions.push(VaultReconciliationAction::UpdatePath {
                    entity_id: known.entity_id.clone(),
                    from_path: known.path.clone(),
                    to_path: file.path.clone(),
                });
                return;
            }
        }

        actions.push(VaultReconciliationAction::Unmanaged {
            path: file.path.clone(),
            reason: VaultConflictReason::unmanaged_file(),
            details: json!({
                "kind": kind,
                "identity_mode": "projection",
            }),
        });
        return;
    }

    actions.push(VaultReconciliationAction::Unmanaged {
        path: file.path.clone(),
        reason: VaultConflictReason::missing_id(),
        details: json!({ "kind": kind }),
    });
}

fn classify_invalid_file(
    file: &ObservedVaultFile,
    reason: VaultConflictReason,
    details: serde_json::Value,
    known_by_path: &HashMap<&str, &VaultFile>,
    covered_known_entities: &mut HashSet<String>,
    actions: &mut Vec<VaultReconciliationAction>,
) {
    if let Some(known) = known_by_path.get(file.path.as_str()) {
        covered_known_entities.insert(known.entity_id.as_str().to_string());
        actions.push(VaultReconciliationAction::Conflict {
            path: file.path.clone(),
            reason,
            entity_id: Some(known.entity_id.clone()),
            observed_entity_id: observed_entity_id(file),
            details,
        });
    } else {
        actions.push(VaultReconciliationAction::Unmanaged {
            path: file.path.clone(),
            reason,
            details,
        });
    }
}

fn classify_managed_files(
    entity_id: &str,
    files: Vec<&ObservedVaultFile>,
    known_by_entity: &HashMap<String, &VaultFile>,
    known_by_path: &HashMap<&str, &VaultFile>,
    covered_known_entities: &mut HashSet<String>,
    actions: &mut Vec<VaultReconciliationAction>,
) {
    let known = known_by_entity.get(entity_id).copied();
    let canonical_path = known
        .and_then(|known| {
            files
                .iter()
                .find(|file| file.path == known.path)
                .map(|file| file.path.as_str())
        })
        .or_else(|| files.first().map(|file| file.path.as_str()));

    if files.len() > 1 {
        classify_duplicate_id_files(
            entity_id,
            files,
            known,
            canonical_path,
            covered_known_entities,
            actions,
        );
        return;
    }

    let file = files[0];
    if let Some(known_at_path) = known_by_path.get(file.path.as_str()) {
        if known_at_path.entity_id.as_str() != entity_id {
            covered_known_entities.insert(known_at_path.entity_id.as_str().to_string());
            actions.push(VaultReconciliationAction::Conflict {
                path: file.path.clone(),
                reason: VaultConflictReason::changed_id(),
                entity_id: Some(known_at_path.entity_id.clone()),
                observed_entity_id: Some(EntityId::from_string(entity_id)),
                details: json!({
                    "expected_entity_id": known_at_path.entity_id.as_str(),
                    "observed_entity_id": entity_id,
                }),
            });
            return;
        }
    }

    match known {
        Some(known) if known.path == file.path => {
            covered_known_entities.insert(known.entity_id.as_str().to_string());
            actions.push(VaultReconciliationAction::KeepSynced {
                entity_id: known.entity_id.clone(),
                path: file.path.clone(),
            });
        }
        Some(known) => {
            covered_known_entities.insert(known.entity_id.as_str().to_string());
            actions.push(VaultReconciliationAction::UpdatePath {
                entity_id: known.entity_id.clone(),
                from_path: known.path.clone(),
                to_path: file.path.clone(),
            });
        }
        None => {
            let kind = observed_kind(file).unwrap_or_else(|| EntityType::new("document::note"));
            actions.push(VaultReconciliationAction::ImportUnknown {
                entity_id: EntityId::from_string(entity_id),
                kind,
                path: file.path.clone(),
            });
        }
    }
}

fn classify_duplicate_id_files(
    entity_id: &str,
    files: Vec<&ObservedVaultFile>,
    known: Option<&VaultFile>,
    canonical_path: Option<&str>,
    covered_known_entities: &mut HashSet<String>,
    actions: &mut Vec<VaultReconciliationAction>,
) {
    if let Some(known) = known {
        covered_known_entities.insert(known.entity_id.as_str().to_string());
    }

    for file in files {
        if Some(file.path.as_str()) == canonical_path {
            match known {
                Some(known) if known.path == file.path => {
                    actions.push(VaultReconciliationAction::KeepSynced {
                        entity_id: known.entity_id.clone(),
                        path: file.path.clone(),
                    });
                }
                Some(known) => {
                    actions.push(VaultReconciliationAction::UpdatePath {
                        entity_id: known.entity_id.clone(),
                        from_path: known.path.clone(),
                        to_path: file.path.clone(),
                    });
                }
                None => {
                    actions.push(VaultReconciliationAction::ImportUnknown {
                        entity_id: EntityId::from_string(entity_id),
                        kind: observed_kind(file)
                            .unwrap_or_else(|| EntityType::new("document::note")),
                        path: file.path.clone(),
                    });
                }
            }
        } else {
            actions.push(VaultReconciliationAction::Conflict {
                path: file.path.clone(),
                reason: VaultConflictReason::duplicate_id(),
                entity_id: known.map(|known| known.entity_id.clone()),
                observed_entity_id: Some(EntityId::from_string(entity_id)),
                details: json!({ "canonical_path": canonical_path }),
            });
        }
    }
}

fn observed_entity_id(file: &ObservedVaultFile) -> Option<EntityId> {
    match &file.identity {
        ObservedVaultIdentity::Managed { entity_id, .. }
        | ObservedVaultIdentity::UnsupportedKind {
            entity_id: Some(entity_id),
            ..
        } => Some(entity_id.clone()),
        _ => None,
    }
}

fn observed_kind(file: &ObservedVaultFile) -> Option<EntityType> {
    match &file.identity {
        ObservedVaultIdentity::Managed { kind, .. } => Some(kind.clone()),
        ObservedVaultIdentity::Unidentified { kind } => kind.clone(),
        ObservedVaultIdentity::UnsupportedKind { kind, .. } => kind.clone(),
        ObservedVaultIdentity::InvalidFrontmatter { .. } => None,
    }
}

fn is_supported_kind(kind: &EntityType, options: &VaultScanOptions) -> bool {
    options
        .supported_kind_prefixes
        .iter()
        .any(|prefix| kind.as_str().starts_with(prefix))
}

fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

pub(crate) fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn modified_millis(time: std::time::SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::types::VaultSyncStatus;

    fn known(entity_id: &str, path: &str) -> VaultFile {
        known_with_hash(entity_id, path, "hash")
    }

    fn known_with_hash(entity_id: &str, path: &str, content_hash: &str) -> VaultFile {
        VaultFile {
            entity_id: EntityId::from_string(entity_id),
            path: path.to_string(),
            file_key: None,
            mtime: None,
            content_hash: content_hash.to_string(),
            frontmatter_hash: None,
            sync_status: VaultSyncStatus::synced(),
            last_seen_at: 0,
        }
    }

    fn managed(entity_id: &str, path: &str) -> ObservedVaultFile {
        ObservedVaultFile {
            path: path.to_string(),
            mtime: None,
            content_hash: "hash".to_string(),
            frontmatter_hash: None,
            identity: ObservedVaultIdentity::Managed {
                entity_id: EntityId::from_string(entity_id),
                kind: EntityType::new("document::note"),
            },
        }
    }

    fn unidentified(path: &str, content_hash: &str) -> ObservedVaultFile {
        ObservedVaultFile {
            path: path.to_string(),
            mtime: None,
            content_hash: content_hash.to_string(),
            frontmatter_hash: None,
            identity: ObservedVaultIdentity::Unidentified { kind: None },
        }
    }

    #[test]
    fn plans_same_id_move() {
        let plan = plan_reconciliation(&[known("ent_1", "old.md")], &[managed("ent_1", "new.md")]);

        assert_eq!(
            plan.actions,
            vec![VaultReconciliationAction::UpdatePath {
                entity_id: EntityId::from_string("ent_1"),
                from_path: "old.md".to_string(),
                to_path: "new.md".to_string(),
            }]
        );
    }

    #[test]
    fn plans_missing_known_file() {
        let plan = plan_reconciliation(&[known("ent_1", "missing.md")], &[]);

        assert_eq!(
            plan.actions,
            vec![VaultReconciliationAction::MarkMissing {
                entity_id: EntityId::from_string("ent_1"),
                path: "missing.md".to_string(),
            }]
        );
    }

    #[test]
    fn treats_changed_id_at_known_path_as_conflict() {
        let plan = plan_reconciliation(
            &[known("ent_old", "doc.md")],
            &[managed("ent_new", "doc.md")],
        );

        assert!(matches!(
            &plan.actions[..],
            [VaultReconciliationAction::Conflict {
                reason,
                entity_id: Some(_),
                observed_entity_id: Some(_),
                ..
            }] if reason.as_str() == "changed_id"
        ));
    }

    #[test]
    fn keeps_prior_canonical_path_for_duplicate_ids() {
        let plan = plan_reconciliation(
            &[known("ent_1", "canonical.md")],
            &[
                managed("ent_1", "copy.md"),
                managed("ent_1", "canonical.md"),
            ],
        );

        assert!(matches!(
            &plan.actions[..],
            [
                VaultReconciliationAction::Conflict { reason, .. },
                VaultReconciliationAction::KeepSynced { .. },
            ] if reason.as_str() == "duplicate_id"
        ));
    }

    #[test]
    fn classifies_unknown_valid_id_for_import() {
        let plan = plan_reconciliation(&[], &[managed("ent_1", "new.md")]);

        assert!(matches!(
            &plan.actions[..],
            [VaultReconciliationAction::ImportUnknown { path, .. }] if path == "new.md"
        ));
    }

    #[test]
    fn classifies_unknown_missing_id_as_unmanaged() {
        let observed = unidentified("loose.md", "hash");
        let plan = plan_reconciliation(&[], &[observed]);

        assert!(matches!(
            &plan.actions[..],
            [VaultReconciliationAction::Unmanaged { reason, .. }]
                if reason.as_str() == "unmanaged_file"
        ));
    }

    #[test]
    fn keeps_known_plain_markdown_synced_by_path_by_default() {
        let plan = plan_reconciliation(
            &[known("ent_1", "doc.md")],
            &[unidentified("doc.md", "new_hash")],
        );

        assert_eq!(
            plan.actions,
            vec![VaultReconciliationAction::KeepSynced {
                entity_id: EntityId::from_string("ent_1"),
                path: "doc.md".to_string(),
            }]
        );
    }

    #[test]
    fn infers_frontmatterless_move_by_unique_hash() {
        let plan = plan_reconciliation(
            &[known_with_hash("ent_1", "old.md", "same_hash")],
            &[unidentified("new.md", "same_hash")],
        );

        assert_eq!(
            plan.actions,
            vec![VaultReconciliationAction::UpdatePath {
                entity_id: EntityId::from_string("ent_1"),
                from_path: "old.md".to_string(),
                to_path: "new.md".to_string(),
            }]
        );
    }

    #[test]
    fn frontmatter_identity_mode_requires_id_for_known_path() {
        let options = VaultScanOptions {
            identity_mode: VaultIdentityMode::Frontmatter,
            ..VaultScanOptions::default()
        };
        let plan = plan_reconciliation_with_options(
            &[known("ent_1", "doc.md")],
            &[unidentified("doc.md", "new_hash")],
            &options,
        );

        assert!(matches!(
            &plan.actions[..],
            [VaultReconciliationAction::Conflict { reason, .. }]
                if reason.as_str() == "missing_id"
        ));
    }
}
