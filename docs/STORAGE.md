# Unified Storage Architecture

**Version:** 1.1
**Architecture:** SQLite + Content-Addressable Filesystem
**Goal:** Local-first storage with enterprise-grade capabilities.

---

## Overview

The unified storage uses:
- **SQLite database** for conversations, messages, threads, documents, and metadata
- **Content-Addressable Storage (CAS)** for binary assets (images, audio, documents)
- **TOML configuration** for user preferences and encrypted API keys
- **Unified data directory** (`~/.local/share/noema`) on all desktop platforms

---

## Directory Structure

### Base Directory

All platforms use the same base directory for simplicity:

```
~/.local/share/noema/           # Base data directory
```

The `PathManager` supports overriding via `PathManager::set_data_dir()` for mobile/embedded platforms.

### Directory Layout

```
~/.local/share/noema/
├── database/
│   └── noema.db              # Main SQLite database
│
├── blob_storage/             # Content-Addressable Storage
│   ├── 7f/                   # Sharded by first 2 chars of SHA-256
│   │   └── 7f8a9b...         # Actual file content (no extension)
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

## Database Schema

### File: `~/.local/share/noema/database/noema.db`

Timestamps are stored as INTEGER (Unix epoch seconds).

### A. Users

Single-tenant by default with "human@noema" user.

```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,                    -- UUID v4
    email TEXT UNIQUE NOT NULL,             -- Default: "human@noema"
    encrypted_anthropic_key TEXT,
    encrypted_openai_key TEXT,
    encrypted_gemini_key TEXT,
    google_oauth_refresh_token TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

### B. Conversations & Threads

Threads enable branching conversations. Each thread forks from a specific span.

```sql
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    title TEXT,
    system_prompt TEXT,
    summary_text TEXT,
    summary_embedding BLOB,                 -- For future vector search
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    conversation_id TEXT REFERENCES conversations(id) ON DELETE CASCADE,
    parent_span_id TEXT REFERENCES spans(id),  -- Fork point (NULL for main thread)
    status TEXT NOT NULL DEFAULT 'active',     -- "active" or "active:name"
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_conversations_user ON conversations(user_id);
CREATE INDEX idx_threads_conversation ON threads(conversation_id);
```

### C. Span-Based Messages

Content is stored in a span-based hierarchy supporting:
- **Parallel model responses**: Multiple AI responses at the same position
- **Editable user input**: User can edit messages, creating alternatives
- **Thread forking**: Fork from any specific span

```sql
-- SpanSets: a position in the conversation
CREATE TABLE span_sets (
    id TEXT PRIMARY KEY,
    thread_id TEXT REFERENCES threads(id) ON DELETE CASCADE,
    sequence_number INTEGER NOT NULL,
    span_type TEXT CHECK(span_type IN ('user', 'assistant')) NOT NULL,
    selected_span_id TEXT,                  -- Currently active span
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_span_sets_thread ON span_sets(thread_id, sequence_number);

-- Spans: one alternative at a position
CREATE TABLE spans (
    id TEXT PRIMARY KEY,
    span_set_id TEXT REFERENCES span_sets(id) ON DELETE CASCADE,
    model_id TEXT,                          -- NULL for user spans
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_spans_span_set ON spans(span_set_id);

-- SpanMessages: individual messages within a span
CREATE TABLE span_messages (
    id TEXT PRIMARY KEY,
    span_id TEXT REFERENCES spans(id) ON DELETE CASCADE,
    sequence_number INTEGER NOT NULL,
    role TEXT CHECK(role IN ('user', 'assistant', 'system', 'tool')) NOT NULL,
    content TEXT NOT NULL,                  -- JSON Array of StoredContent
    text_content TEXT,                      -- Extracted plain text for search
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_span_messages_span ON span_messages(span_id, sequence_number);
```

**Hierarchy:**
```
Conversation
  └── Thread (main, or forked from a span)
        └── SpanSet (position 1, 2, 3...)
              └── Span (alternative A, B, C...)
                    └── SpanMessage (for multi-turn within one "response")
```

### D. Assets (CAS Metadata)

```sql
CREATE TABLE assets (
    id TEXT PRIMARY KEY,                    -- SHA-256 hash
    mime_type TEXT NOT NULL,
    original_filename TEXT,
    file_size_bytes INTEGER,
    metadata_json TEXT,
    local_path TEXT,                        -- Relative path in blob_storage/
    created_at INTEGER NOT NULL
);
```

### E. Documents (Episteme-compatible)

For RAG and document management:

```sql
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    source TEXT NOT NULL,                   -- 'google_drive', 'ai_generated', 'user_created'
    source_id TEXT,                         -- External ID (e.g., Google Doc ID)
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE document_tabs (
    id TEXT PRIMARY KEY,
    document_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
    parent_tab_id TEXT REFERENCES document_tabs(id) ON DELETE CASCADE,
    tab_index INTEGER NOT NULL,
    title TEXT NOT NULL,
    icon TEXT,
    content_markdown TEXT,
    referenced_assets TEXT,                 -- JSON array of asset IDs
    source_tab_id TEXT,
    current_revision_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE document_revisions (
    id TEXT PRIMARY KEY,
    tab_id TEXT REFERENCES document_tabs(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL,
    parent_revision_id TEXT REFERENCES document_revisions(id) ON DELETE SET NULL,
    content_markdown TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    referenced_assets TEXT,
    created_at INTEGER NOT NULL,
    created_by TEXT NOT NULL DEFAULT 'import'
);

CREATE INDEX idx_documents_user ON documents(user_id);
CREATE INDEX idx_documents_source ON documents(source);
CREATE INDEX idx_documents_user_source_id ON documents(user_id, source, source_id);
CREATE INDEX idx_document_tabs_document ON document_tabs(document_id);
CREATE INDEX idx_document_tabs_parent ON document_tabs(parent_tab_id);
CREATE INDEX idx_document_revisions_tab ON document_revisions(tab_id);
```

---

## Content Block Types

The `content` column in `span_messages` contains a JSON array of `StoredContent` blocks:

```rust
// Defined in noema-core/src/storage/content.rs
enum StoredContent {
    Text { text: String },
    Image { data: String, mime_type: String },      // Base64 inline
    Audio { data: String, mime_type: String },      // Base64 inline
    AssetRef { asset_id: String, mime_type: String, filename: Option<String> },
    DocumentRef { id: String, title: String },      // For RAG
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}
```

Example JSON:
```json
[
  {"type": "text", "text": "Hello, world!"},
  {"type": "asset_ref", "asset_id": "7f8a9b...", "mime_type": "image/png"},
  {"type": "document_ref", "id": "doc-123", "title": "Meeting Notes"}
]
```

---

## Content-Addressable Storage (CAS)

### Implementation: `BlobStore`

Located in `noema-core/src/storage/blob.rs`:

```rust
pub struct BlobStore {
    root: PathBuf,  // ~/.local/share/noema/blob_storage
}

impl BlobStore {
    pub fn store(&self, data: &[u8]) -> io::Result<StoredBlob>;
    pub fn get(&self, hash: &str) -> io::Result<Vec<u8>>;
    pub fn exists(&self, hash: &str) -> bool;
    pub fn delete(&self, hash: &str) -> io::Result<bool>;
    pub fn verify(&self, hash: &str) -> io::Result<bool>;
    pub fn compute_hash(data: &[u8]) -> String;
}
```

### Storage Flow

1. **Store**: Compute SHA-256 → check if exists → write atomically via temp file
2. **Retrieve**: Look up by hash in sharded directory
3. **Deduplication**: Same content = same hash = single file

### Directory Sharding

```
blob_storage/
├── 7f/
│   ├── 7f8a9bc4d5e6f7...
│   └── 7fab12cd34ef56...
├── a1/
│   └── a1b2c3d4e5f6a7...
└── ff/
    └── ff00112233445566...
```

---

## Configuration

### Settings File

**File:** `~/.local/share/noema/config/settings.toml`

```toml
# User email for database identification
user_email = "user@example.com"

# Default model ID
default_model = "claude/models/claude-sonnet-4-5-20250929"

# Encrypted API keys (provider -> encrypted value)
[api_keys]
anthropic = "encrypted:base64..."
openai = "encrypted:base64..."

# Favorite models for quick access
favorite_models = [
    "claude/claude-sonnet-4-5",
    "openai/gpt-4o"
]
```

### API Key Encryption

API keys are encrypted using AES-256-GCM before storage:

```rust
// config/src/crypto.rs
pub fn encrypt_string(plaintext: &str) -> Result<String, String>;
pub fn decrypt_string(ciphertext: &str) -> Result<String, String>;
```

---

## Path Manager API

```rust
use config::PathManager;

// Set custom data directory (for mobile/embedded)
PathManager::set_data_dir(path);
PathManager::set_log_file(path);

// Base directories
PathManager::data_dir()           // ~/.local/share/noema
PathManager::config_dir()         // Same as data_dir
PathManager::cache_dir()          // data_dir/cache
PathManager::logs_dir()           // data_dir/logs
PathManager::log_file_path()      // data_dir/logs/noema.log

// Database
PathManager::database_dir()       // data_dir/database
PathManager::db_path()            // data_dir/database/noema.db

// Content-Addressable Storage
PathManager::blob_storage_dir()   // data_dir/blob_storage
PathManager::blob_path(hash)      // data_dir/blob_storage/{hash[0:2]}/{hash}

// Configuration
PathManager::config_subdir()      // data_dir/config
PathManager::settings_path()      // data_dir/config/settings.toml
PathManager::env_path()           // data_dir/config/.env
PathManager::mcp_config_path()    // data_dir/mcp.toml

// Models
PathManager::models_dir()         // data_dir/models
PathManager::whisper_model_path() // data_dir/models/ggml-base.en.bin

// Create all directories
PathManager::ensure_dirs_exist()?;
```

---

## Storage Traits

### SessionStore

```rust
// noema-core/src/storage/traits.rs
#[async_trait]
pub trait SessionStore: Send {
    type Transaction: StorageTransaction;

    fn messages(&self) -> &[ChatMessage];
    fn messages_mut(&mut self) -> &mut Vec<ChatMessage>;
    fn begin(&self) -> Self::Transaction;
    async fn commit(&mut self, tx: Self::Transaction) -> Result<()>;
    async fn clear(&mut self) -> Result<()>;

    // For parallel model execution
    async fn commit_parallel_responses(
        &mut self,
        responses: &[(String, Vec<ChatMessage>)],
        selected_index: usize,
    ) -> Result<(String, Vec<String>)>;
}
```

### Implementations

- **`MemorySession`**: In-memory storage (no persistence)
- **`SqliteSession`**: SQLite-backed persistent storage

---

## SqliteStore API

```rust
// noema-core/src/storage/sqlite.rs
impl SqliteStore {
    // Creation
    pub fn open(path: impl AsRef<Path>) -> Result<Self>;
    pub fn in_memory() -> Result<Self>;

    // Users
    pub fn get_or_create_default_user(&self) -> Result<UserInfo>;
    pub fn get_or_create_user_by_email(&self, email: &str) -> Result<UserInfo>;

    // Conversations
    pub fn create_conversation(&self, user_id: &str) -> Result<SqliteSession>;
    pub async fn open_conversation<F>(&self, id: &str, resolver: F) -> Result<SqliteSession>;

    // Documents
    pub fn create_document(&self, user_id: &str, title: &str, source: DocumentSource, source_id: Option<&str>) -> Result<String>;
    pub fn get_document(&self, id: &str) -> Result<Option<DocumentInfo>>;
    pub fn list_documents(&self, user_id: &str) -> Result<Vec<DocumentInfo>>;
    pub fn search_documents(&self, user_id: &str, query: &str, limit: usize) -> Result<Vec<DocumentInfo>>;

    // Document Tabs & Revisions
    pub fn create_document_tab(&self, ...) -> Result<String>;
    pub fn create_document_revision(&self, ...) -> Result<String>;

    // Span-based operations
    pub fn create_span_set(&self, thread_id: &str, span_type: SpanType) -> Result<String>;
    pub fn create_span(&self, span_set_id: &str, model_id: Option<&str>) -> Result<String>;
    pub fn add_span_message(&self, span_id: &str, role: Role, content: &StoredPayload) -> Result<String>;
    pub fn set_selected_span(&self, span_set_id: &str, span_id: &str) -> Result<()>;

    // Threading
    pub fn create_fork_thread(&self, conversation_id: &str, parent_span_id: &str, name: Option<&str>) -> Result<String>;
    pub fn create_fork_conversation(&self, user_id: &str, parent_span_id: &str, name: Option<&str>) -> Result<(String, String)>;
    pub fn get_thread_messages_with_ancestry(&self, thread_id: &str) -> Result<Vec<StoredMessage>>;

    // Assets
    pub fn register_asset(&self, hash: &str, mime_type: &str, ...) -> Result<()>;
    pub fn get_asset(&self, hash: &str) -> Result<Option<AssetInfo>>;
}
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

## Security Considerations

| Asset | Protection |
|-------|------------|
| API Keys | AES-256-GCM encryption in settings.toml |
| `.env` file | File permissions (600) |
| OAuth tokens | Stored in users table |
| Blob storage | Files named by hash (content-blind) |

### Recommendations

1. **Never commit** `settings.toml` with secrets to version control
2. **Enable disk encryption** on the host system
3. **Set restrictive permissions** on the config directory
