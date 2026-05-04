# Vault-Backed Markdown

**Status:** Proposal
**Created:** 2026-05-04
**Related:** [UNIFIED_CONTENT_MODEL.md](UNIFIED_CONTENT_MODEL.md)

---

## Goal

Make human-authored document content portable, inspectable, and editable as normal
Markdown files while preserving SQLite as Noema's graph, index, access, and runtime
database.

This extends the Unified Content Model. It does not replace it. Entities keep stable
identity and relations; vault files become the canonical body storage for managed
`document::*` content.

## Source Of Truth Split

| Data | Canonical Store |
|------|-----------------|
| Human-authored Markdown bodies | Vault `.md` files |
| Entity identity, type, owner, privacy policy | SQLite `entities` |
| Relations, directories, tags, backlinks | SQLite `entity_relations` |
| Search and embedding chunks | SQLite projection from current content |
| Conversations, turns, spans, messages, tool state | SQLite |
| Asset metadata and blob reachability | SQLite projection plus blob store |

`content_blocks` remain useful as immutable snapshots. Existing APIs and RAG can keep
keying off `content_block_id`; vault-backed writes create a fresh snapshot after the
file write succeeds.

## Invariants

- One markdown-backed entity has at most one canonical vault file.
- Entity ID is identity. Path and filename are mutable presentation.
- File watcher events are hints. Startup/full vault scans are authoritative.
- Frontmatter is a sync interface, not a trust boundary.
- System-owned metadata is validated before it changes SQLite state.
- Conflicts are explicit states. Noema should not merge, delete, or reassign identity
  automatically when identity is ambiguous.

## Frontmatter

Each managed file carries stable identity in YAML frontmatter:

```yaml
---
id: ent_...
kind: document::note
origin: google_drive:...
privacy: private
tags: []
---
```

Field ownership:

| Field | Ownership | Notes |
|-------|-----------|-------|
| `id` | System-owned | Portable identity marker. Edits become reconciliation conflicts. |
| `kind` | System-owned | Entity type. User-facing conversion should go through Noema. |
| `origin` | System-owned | Imported/source identity. Edits are validated, not blindly trusted. |
| `privacy` | Policy-controlled | Parsed as a requested change and checked through access policy. |
| `title` | User-editable | Can sync to `entities.name`. |
| `tags` | User-editable | Can project to tag relations or metadata. |
| Other keys | User-editable metadata | Preserved unless reserved by Noema. |

## Projection Tables

The vault projection tracks observed files and reconciliation state:

```sql
CREATE TABLE vault_files (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE,
    file_key TEXT,
    mtime INTEGER,
    content_hash TEXT NOT NULL,
    frontmatter_hash TEXT,
    sync_status TEXT NOT NULL,
    last_seen_at INTEGER NOT NULL
);

CREATE TABLE vault_conflicts (
    id TEXT PRIMARY KEY,
    entity_id TEXT,
    path TEXT NOT NULL,
    reason TEXT NOT NULL,
    observed_entity_id TEXT,
    details TEXT,
    created_at INTEGER NOT NULL
);
```

`file_key` can store platform file identity when available, but it is only an
optimization. It is not portable across sync tools, copies, volume moves, or cloud
drive restores.

## Reconciliation Rules

The scanner compares vault files, frontmatter, file keys, hashes, and the SQLite
projection.

| Observation | Action |
|-------------|--------|
| Same `id`, new path | Update `vault_files.path`. |
| Known path/file with removed `id` | Conflict; offer to restore the original ID. |
| Known path/file with changed `id` | Conflict; do not mutate entity identity. |
| Multiple files with same `id` | Keep prior canonical path if present; mark extras as duplicate-ID conflicts. |
| Unknown file with valid unknown `id` | Import or bind, depending policy. |
| Unknown file without `id` | Leave unmanaged or assign ID through explicit import. |
| Previously known file missing | Mark missing. Delete/archive only after explicit policy or user action. |

Duplicate IDs should default to "fork as new document" when the user copied a file.
Replacing the canonical file should require either a missing old path or explicit
confirmation.

## Write Path

Vault-backed document updates go through one coordinator:

1. Validate access and requested metadata changes.
2. Serialize frontmatter from canonical entity state.
3. Write to a temporary file inside the vault.
4. Rename atomically over the canonical path.
5. Re-read or hash the final file.
6. Store a new `content_blocks` snapshot.
7. Update `entities.content_block_id` and `vault_files`.
8. Enqueue embedding and asset-reference projection updates.

Hash-based conflict detection should prevent Noema from overwriting an externally
edited file with stale editor state.

## Scanner And Watcher

The scanner is the only place that classifies vault state. The watcher only queues
paths and asks the scanner to rescan affected areas. This keeps closed-app edits,
sync-tool changes, renames, duplicate copies, and missed events on the same code path.

## Non-Goals

- Replacing SQLite with files.
- Moving chat turns, tool state, conversations, or access policy into Markdown.
- Trusting frontmatter privacy or ownership fields without daemon-side validation.
- Supporting multiple canonical files for one entity.
