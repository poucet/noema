# Storage Architecture

**Status:** Draft
**Version:** 2.0 (Unified Content Model)
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)

---

## Overview

Storage implements the [Unified Content Model](UNIFIED_CONTENT_MODEL.md) three-layer architecture using SQLite + content-addressable blob storage.

| Layer | Purpose | Key Concepts |
|-------|---------|--------------|
| **Addressable** | Unified identity, naming, relationships | Entities, entity relations |
| **Structure** | Domain-specific organization | Views, turns, spans, messages, documents, revisions |
| **Content** | Immutable content storage | Content blocks (text with origin), assets + blobs (binary) |

**Core Principle**: Content is heavy and immutable. Structure is lightweight and mutable. Identity is addressable and organizational.

---

## Addressable Layer

All addressable things (views, documents, assets) are **entities** with unified identity.

- **Entities** have an id, type, name, slug (for @mentions), privacy, and archive flags
- **Entity relations** link entities with typed relationships: `forked_from`, `spawned_from`, `references` — each with optional metadata (e.g., `{at_turn_id}` for forks)

---

## Content Layer

### Content Blocks (Text)

Textual content with origin tracking. Each block is unique — NOT deduplicated — for provenance.

- Identified by UUID, integrity-checked via SHA-256 hash
- Content type: `text/plain`, `text/markdown`, `text/typst`
- Origin tracking: who created it (user, assistant, system, import), which model, parent block

### Assets (Binary)

Binary content metadata + content-addressable blob storage.

- Asset metadata in SQLite (mime type, filename, size)
- Actual bytes in blob storage, sharded by SHA-256 hash prefix
- Deduplication at the blob level (same content = same hash = stored once)

---

## Structure Layer: Conversations

Conversations use a **Turn → Span → Message** hierarchy with **Views** selecting paths.

```
Entity (addressable identity)
  └── View (path through conversation)
        └── View Selections (turn → span mappings)
              └── Turn (position in sequence, shared across views)
                    └── Span (one alternative response)
                          └── Message (individual content piece)
                                └── Content Block / Asset refs
```

| Use Case | How It Works |
|----------|--------------|
| **Parallel models** | Multiple spans at same turn, view selects one |
| **Regenerate** | Add new span at turn, select it |
| **Fork** | New view sharing selections up to fork point |
| **Edit & splice** | New span at turn, can reuse subsequent turns |
| **Subconversations** | Child view with `spawned_from` relation to parent |

---

## Structure Layer: Documents

Documents with tabs and per-tab revision history.

- **Documents** belong to an entity, have a title and source (user-created, AI-generated, imported)
- **Tabs** form a tree (parent tab → child tabs), each with its own revision chain
- **Revisions** link to content blocks, forming a linear history per tab

---

## Storage Traits

The storage layer uses traits for abstraction, enabling SQLite, in-memory, and mock implementations.

| Trait | Purpose |
|-------|---------|
| `EntityStore` | Entity CRUD and relations |
| `ContentBlockStore` | Text content with origin |
| `AssetStore` | Binary asset metadata |
| `BlobStore` | Binary content storage (content-addressable filesystem) |
| `TurnStore` | Turn/span/message operations |
| `DocumentStore` | Document/tab/revision operations |
| `UserStore` | User management |

All stores are bundled via a `StorageTypes` trait for dependency injection.

---

## Session API

The `Session` manages conversation state with lazy content resolution.

| Operation | Description |
|-----------|-------------|
| `open` / `create` | Open existing or create new conversation |
| `messages_for_display` / `messages_for_llm` | Get messages with resolved content |
| `commit` | Persist new messages to a conversation |
| `commit_parallel` | Persist responses from multiple models |
| `fork` | Create new view branching at a given turn |
| `select_span` | Switch which span a view uses at a turn |
| `spawn_subconversation` | Create child view with inherited context |

---

## Security

| Asset | Protection |
|-------|------------|
| API Keys | AES-256-GCM encryption |
| Private content | `is_private` flag blocks cloud models |
| Blob storage | Files named by hash (content-blind) |
