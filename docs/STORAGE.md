# Noema Storage Architecture

**Version:** 2.0 (Unified Content Model)
**Architecture:** SQLite + Content-Addressable Filesystem + Three-Layer Model

---

## Overview

Noema 0.2 introduces the **Unified Content Model (UCM)** - a three-layer architecture separating content from structure from identity.

| Layer | Purpose | Key Tables |
|-------|---------|------------|
| **Addressable** | Unified identity, naming, relationships | `entities`, `entity_relations` |
| **Structure** | Domain-specific organization | `views`, `turns`, `spans`, `messages`, `documents`, `revisions` |
| **Content** | Immutable content storage | `content_blocks`, `assets` + blob filesystem |

**Core Principle**: Content is heavy and immutable. Structure is lightweight and mutable. Identity is addressable and organizational.

---

## Directory Structure

```
~/.local/share/noema/
├── database/
│   └── noema.db              # Main SQLite database
│
├── blob_storage/             # Content-Addressable Storage (CAS)
│   ├── 7f/                   # Sharded by first 2 chars of SHA-256
│   │   └── 7f8a9b...         # Binary content (no extension)
│   └── a1/
│       └── a1b2c3...
│
├── config/
│   ├── settings.toml         # User preferences & encrypted API keys
│   └── .env                  # Optional: environment overrides
│
├── logs/
│   └── noema.log             # Application logs
│
├── cache/                    # Temporary cached data
│
└── models/
    └── ggml-base.en.bin      # Whisper voice model (~140MB)
```

---

## Addressable Layer

All addressable things (views, documents, assets) are entities with unified identity.

### Entities Table

```sql
CREATE TABLE entities (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,     -- 'view', 'document', 'asset'
    user_id TEXT REFERENCES users(id),
    name TEXT,
    slug TEXT UNIQUE,              -- for @mentions
    is_private INTEGER DEFAULT 0,
    is_archived INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

### Entity Relations

Relationships between entities (fork ancestry, references, spawned conversations):

```sql
CREATE TABLE entity_relations (
    from_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    to_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation TEXT NOT NULL,        -- 'forked_from', 'spawned_from', 'references'
    metadata TEXT,                 -- JSON (e.g., {at_turn_id: "..."})
    created_at INTEGER NOT NULL,
    PRIMARY KEY (from_id, to_id, relation)
);
```

**Relation Types**:

| Relation | From | To | Use Case |
|----------|------|----|----------|
| `forked_from` | View | View | Fork ancestry with `{at_turn_id}` metadata |
| `spawned_from` | View | View | Subconversation parent with `{span_id}` metadata |
| `references` | Any | Any | Cross-references, citations |

---

## Content Layer

### Content Blocks (Text)

All textual content with origin tracking. NOT deduplicated - each block is unique for provenance tracking.

```sql
CREATE TABLE content_blocks (
    id TEXT PRIMARY KEY,           -- UUID
    content_hash TEXT NOT NULL,    -- SHA-256 (for integrity, not dedup)
    content_type TEXT NOT NULL,    -- text/plain, text/markdown, text/typst
    text TEXT NOT NULL,
    origin_kind TEXT NOT NULL,     -- user, assistant, system, import
    origin_user_id TEXT,
    origin_model_id TEXT,
    origin_source_id TEXT,
    origin_parent_id TEXT,
    is_private INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);
```

### Assets (Binary)

Binary content metadata. Actual bytes stored in `blob_storage/` via BlobStore.

```sql
CREATE TABLE assets (
    id TEXT PRIMARY KEY,           -- UUID
    blob_hash TEXT NOT NULL,       -- SHA-256 (content-addressed in blob store)
    mime_type TEXT NOT NULL,
    filename TEXT,
    size_bytes INTEGER NOT NULL,
    is_private INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);
```

---

## Structure Layer: Conversations

Conversations use a **Turn → Span → Message** hierarchy with **Views** selecting paths.

### Hierarchy

```
Entity (addressable identity)
  └── View (path through conversation)
        └── View Selections (turn → span mappings)
              └── Turn (position in sequence, shared across views)
                    └── Span (one alternative response)
                          └── Message (individual content piece)
                                └── Content Block / Asset refs
```

### Tables

```sql
-- Views reference entities
CREATE TABLE views (
    id TEXT PRIMARY KEY REFERENCES entities(id) ON DELETE CASCADE
);

-- Turns are shared positions in a conversation
CREATE TABLE turns (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    sequence_number INTEGER NOT NULL,
    role TEXT NOT NULL,            -- 'user' or 'assistant'
    created_at INTEGER NOT NULL
);

-- Spans are alternatives at a turn
CREATE TABLE spans (
    id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL REFERENCES turns(id),
    model_id TEXT,                 -- NULL for user spans
    is_complete INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);

-- Messages within a span
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    span_id TEXT NOT NULL REFERENCES spans(id),
    sequence_number INTEGER NOT NULL,
    role TEXT NOT NULL,            -- user, assistant, system, tool
    content_id TEXT REFERENCES content_blocks(id),
    tool_calls TEXT,               -- JSON array
    tool_results TEXT,             -- JSON array
    created_at INTEGER NOT NULL
);

-- View selections form the path
CREATE TABLE view_selections (
    view_id TEXT NOT NULL REFERENCES views(id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL REFERENCES turns(id),
    span_id TEXT NOT NULL REFERENCES spans(id),
    sequence_number INTEGER NOT NULL,
    PRIMARY KEY (view_id, turn_id)
);
```

### Use Cases Enabled

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

```sql
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    entity_id TEXT REFERENCES entities(id),
    title TEXT NOT NULL,
    source TEXT NOT NULL,          -- user_created, ai_generated, imported
    source_id TEXT,
    current_tab_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE document_tabs (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id),
    parent_tab_id TEXT REFERENCES document_tabs(id),
    tab_index INTEGER NOT NULL,
    title TEXT NOT NULL,
    current_revision_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE revisions (
    id TEXT PRIMARY KEY,
    tab_id TEXT NOT NULL REFERENCES document_tabs(id),
    parent_revision_id TEXT REFERENCES revisions(id),
    revision_number INTEGER NOT NULL,
    content_id TEXT NOT NULL REFERENCES content_blocks(id),
    created_by TEXT NOT NULL,      -- user_id or 'import'
    created_at INTEGER NOT NULL
);
```

---

## Users

Single-tenant by default with "human@noema" user.

```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    encrypted_anthropic_key TEXT,
    encrypted_openai_key TEXT,
    encrypted_gemini_key TEXT,
    google_oauth_refresh_token TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

---

## Content-Addressable Storage (BlobStore)

### Implementation

```rust
pub struct BlobStore {
    root: PathBuf,  // ~/.local/share/noema/blob_storage
}

impl BlobStore {
    pub fn store(&self, data: &[u8]) -> io::Result<BlobHash>;
    pub fn get(&self, hash: &BlobHash) -> io::Result<Vec<u8>>;
    pub fn exists(&self, hash: &BlobHash) -> bool;
    pub fn delete(&self, hash: &BlobHash) -> io::Result<bool>;
}
```

### Directory Sharding

```
blob_storage/
├── 7f/
│   ├── 7f8a9bc4d5e6f7...
│   └── 7fab12cd34ef56...
└── a1/
    └── a1b2c3d4e5f6a7...
```

---

## Storage Traits

The storage layer uses traits for abstraction with SQLite, memory, and mock implementations.

### Core Traits

| Trait | Purpose |
|-------|---------|
| `EntityStore` | Entity CRUD and relations |
| `ContentBlockStore` | Text content with origin |
| `AssetStore` | Binary asset metadata |
| `BlobStore` | Binary content storage |
| `TurnStore` | Turn/span/message operations |
| `ConversationStore` | Conversation operations (deprecated, use EntityStore) |
| `DocumentStore` | Document/tab/revision operations |
| `UserStore` | User management |

### StorageTypes Bundle

All stores bundled for dependency injection:

```rust
pub trait StorageTypes: Clone + Send + Sync + 'static {
    type Blob: BlobStore;
    type ContentBlock: ContentBlockStore;
    type Asset: AssetStore;
    type Turn: TurnStore;
    type Conversation: ConversationStore;
    type Document: DocumentStore;
    type User: UserStore;
    type Entity: EntityStore;
}
```

---

## Session API

The `Session<S: StorageTypes>` manages conversation state with lazy content resolution.

```rust
impl<S: StorageTypes> Session<S> {
    // Open existing or create new
    pub fn open(stores: &S, conversation_id: &str, view_id: Option<&str>) -> Result<Self>;
    pub fn create(stores: &S, user_id: &str) -> Result<Self>;

    // Get messages with resolved content
    pub fn messages_for_display(&self) -> Vec<ResolvedMessage>;
    pub fn messages_for_llm(&self) -> Vec<ResolvedMessage>;

    // Commit changes
    pub fn commit(&mut self, messages: Vec<NewMessage>) -> Result<()>;
    pub fn commit_parallel(&mut self, responses: Vec<(String, Vec<NewMessage>)>) -> Result<Vec<String>>;

    // View operations
    pub fn fork(&mut self, at_turn: &TurnId) -> Result<ViewId>;
    pub fn select_span(&mut self, turn: &TurnId, span: &SpanId) -> Result<()>;

    // Subconversations
    pub fn spawn_subconversation(&mut self, context: Vec<NewMessage>) -> Result<ViewId>;
}
```

---

## Path Manager API

```rust
use config::PathManager;

// Base directories
PathManager::data_dir()           // ~/.local/share/noema
PathManager::database_dir()       // data_dir/database
PathManager::db_path()            // data_dir/database/noema.db
PathManager::blob_storage_dir()   // data_dir/blob_storage
PathManager::config_subdir()      // data_dir/config
PathManager::settings_path()      // data_dir/config/settings.toml
PathManager::models_dir()         // data_dir/models
PathManager::ensure_dirs_exist()?;
```

---

## Backup & Restore

### Full Backup

```bash
tar -czf noema-backup.tar.gz \
  ~/.local/share/noema/database/ \
  ~/.local/share/noema/blob_storage/ \
  ~/.local/share/noema/config/
```

### Database-Only Backup

```bash
sqlite3 ~/.local/share/noema/database/noema.db ".backup backup.db"
```

---

## Security

| Asset | Protection |
|-------|------------|
| API Keys | AES-256-GCM encryption in settings.toml |
| Private content | `is_private` flag blocks cloud models |
| OAuth tokens | Stored in users table |
| Blob storage | Files named by hash (content-blind) |

---

## Migration from 1.x

The UCM introduces new tables while preserving data:

| Old | New | Migration |
|-----|-----|-----------|
| `conversations` | `entities` + `views` | Entity created, view linked |
| `threads` | N/A (removed) | Single main thread per conversation |
| `span_sets` | `turns` | Renamed, simplified |
| `spans` | `spans` | Turn FK changed |
| `span_messages` | `messages` | Content moved to content_blocks |

Legacy tables are preserved with `legacy_` prefix during migration.
