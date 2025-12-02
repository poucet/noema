# Unified Storage Migration Plan

Migrate Noema from Base64 inline storage to the unified architecture with Content-Addressable Storage (CAS), users table, threads, and vector embeddings.

---

## Roadmap Items (Existing)

- [ ] Add RAG
- [ ] Add Search
- [ ] Add embeddings
- [ ] Add document storage with import from google docs
- [ ] Test pdf extraction

---

## Phase 1: Foundation (PathManager + Types)

### 1.1 Update PathManager (`config/src/paths.rs`)

Add new path methods:
- `database_dir()` → `data_dir/database/`
- `blob_storage_dir()` → `data_dir/blob_storage/`
- `settings_path()` → `data_dir/config/settings.toml`
- `env_path()` → `data_dir/config/.env`
- `blob_path(hash: &str)` → `blob_storage/{hash[0:2]}/{hash}`

Update `db_path()` to return `database_dir/noema.db`.

### 1.2 Add Blob Storage Module (`noema-core/src/storage/blob.rs`)

New module for CAS operations:
```rust
pub struct BlobStore { root: PathBuf }

impl BlobStore {
    pub fn new(root: PathBuf) -> Self;
    pub fn store(&self, data: &[u8]) -> Result<String>;  // Returns SHA-256 hash
    pub fn get(&self, hash: &str) -> Result<Vec<u8>>;
    pub fn exists(&self, hash: &str) -> bool;
    pub fn delete(&self, hash: &str) -> Result<()>;
    pub fn path_for(&self, hash: &str) -> PathBuf;
}
```

### 1.3 Update ContentBlock (`noema-core/llm/src/api.rs`)

Extend ContentBlock enum to support blob references:
```rust
pub enum ContentBlock {
    Text { text: String },
    // Legacy inline (for migration compatibility)
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
    // New blob references
    ImageRef { asset_id: String, mime_type: String },
    AudioRef { asset_id: String, mime_type: String },
    DocumentRef { asset_id: String, mime_type: String, filename: Option<String> },
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}
```

Add helper methods:
- `ContentBlock::is_blob_ref() -> bool`
- `ContentBlock::asset_id() -> Option<&str>`
- `ContentBlock::to_blob_ref(hash: String) -> ContentBlock` (convert inline to ref)

---

## Phase 2: Database Schema

### 2.1 New Schema (`noema-core/src/storage/sqlite.rs`)

Replace `init_schema()` with new unified schema:

```sql
-- Users (single-tenant default)
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    encrypted_anthropic_key TEXT,
    encrypted_openai_key TEXT,
    encrypted_gemini_key TEXT,
    google_oauth_refresh_token TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Conversations (add user_id, system_prompt, embeddings)
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id),
    title TEXT,
    system_prompt TEXT,
    summary_text TEXT,
    summary_embedding BLOB,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Threads (branching support)
CREATE TABLE IF NOT EXISTS threads (
    id TEXT PRIMARY KEY,
    conversation_id TEXT REFERENCES conversations(id) ON DELETE CASCADE,
    parent_message_id TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Messages (thread-based, with embeddings)
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    thread_id TEXT REFERENCES threads(id) ON DELETE CASCADE,
    role TEXT CHECK(role IN ('user', 'assistant', 'system')),
    content_json TEXT NOT NULL,
    text_content TEXT,
    embedding BLOB,
    provider TEXT,
    model TEXT,
    tokens_used INTEGER,
    position INTEGER NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Assets (CAS metadata)
CREATE TABLE IF NOT EXISTS assets (
    id TEXT PRIMARY KEY,
    mime_type TEXT NOT NULL,
    original_filename TEXT,
    file_size_bytes INTEGER,
    metadata_json TEXT,
    local_path TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_threads_conversation ON threads(conversation_id);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id, position);
```

### 2.2 Schema Version Table

Add migration tracking:
```sql
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### 2.3 Migration Function

Add `migrate_schema()` that:
1. Checks current schema version
2. Applies incremental migrations
3. Handles v0 (old schema) → v1 (new schema) migration

---

## Phase 3: Storage Layer Refactor

### 3.1 Update Storage Traits (`noema-core/src/storage/traits.rs`)

Extend `SessionStore` trait:
```rust
#[async_trait]
pub trait SessionStore: Send {
    // Existing methods...

    // New blob operations
    fn blob_store(&self) -> Option<&BlobStore>;
    async fn store_blob(&self, data: &[u8], mime_type: &str) -> Result<String>;
    async fn get_blob(&self, hash: &str) -> Result<Vec<u8>>;
}
```

### 3.2 Refactor SqliteStore (`noema-core/src/storage/sqlite.rs`)

Major changes:
1. Add `BlobStore` field to `SqliteStore`
2. Update `create_conversation()` to also create default thread
3. Update `write_messages()` to:
   - Extract blob data from ContentBlocks
   - Store blobs via BlobStore
   - Convert inline blocks to refs
   - Insert asset records
   - Store message with refs only
4. Update `read_messages()` to handle both inline and ref formats
5. Add `get_or_create_user()` for default user initialization

### 3.3 Update SqliteSession

- Add `thread_id` field alongside `conversation_id`
- Update queries to join through threads table
- Maintain backward compatibility during migration

---

## Phase 4: Message Handling

### 4.1 Update Attachment Processing (`noema-ext/src/attachments.rs`)

Modify `process_attachment()`:
- Accept `BlobStore` reference
- Decode Base64 input
- Store blob and get hash
- Return `ContentBlock::ImageRef` / `AudioRef` instead of inline

### 4.2 Update Chat Commands (`noema-ui/src-tauri/src/commands/chat.rs`)

Modify `send_message_with_attachments()`:
- Pass blob store to attachment processing
- Ensure all attachments become blob refs before storage

### 4.3 Update Display Types (`noema-ui/src-tauri/src/types.rs`)

Update `DisplayContent` to handle blob refs:
- Add variant for blob URLs/paths
- Frontend can fetch blobs via separate endpoint or embed on demand

---

## Phase 5: Initialization

### 5.1 Update App Init (`noema-ui/src-tauri/src/commands/init.rs`)

Modify `init_storage()`:
1. Create all directories (database/, blob_storage/, config/)
2. Open database with new schema
3. Run migrations if needed
4. Create default "owner@local" user if not exists
5. Initialize BlobStore

### 5.2 Add Blob Serving Endpoint

New Tauri command for serving blobs:
```rust
#[tauri::command]
pub async fn get_blob(state: State<'_, AppState>, hash: String) -> Result<Vec<u8>, String>
```

---

## Phase 6: Migration Tool

### 6.1 Data Migration (`noema-core/src/storage/migrate.rs`)

New module for migrating existing data:

```rust
pub async fn migrate_v0_to_v1(
    old_db: &Path,
    new_db: &Path,
    blob_store: &BlobStore,
) -> Result<MigrationStats>
```

Steps:
1. Open old database (read-only)
2. Create new database with new schema
3. Create default user
4. For each conversation:
   - Copy conversation record (add user_id)
   - Create default thread
   - For each message:
     - Parse old payload JSON
     - Extract inline blobs → store in BlobStore → get hash
     - Convert ContentBlocks to refs
     - Insert message with new format
5. Return stats (conversations, messages, blobs migrated)

### 6.2 Migration Command

Add CLI or Tauri command to trigger migration:
```rust
#[tauri::command]
pub async fn migrate_database(state: State<'_, AppState>) -> Result<MigrationStats, String>
```

---

## Phase 7: Configuration Unification

### 7.1 Settings File (`config/src/settings.rs`)

New module for `settings.toml`:
```rust
#[derive(Serialize, Deserialize)]
pub struct Settings {
    pub storage: StorageSettings,
    pub ui: UiSettings,
    pub models: ModelSettings,
    pub mcp: McpSettings,
}
```

### 7.2 Update MCP Config (`noema-core/src/mcp/config.rs`)

- Move server definitions to `settings.toml` under `[mcp]`
- Keep OAuth tokens encrypted in database (users table or separate credentials table)
- Remove plaintext token storage from TOML

---

## Implementation Order

```
1. config/src/paths.rs                    # New path methods
2. noema-core/src/storage/blob.rs         # New BlobStore
3. noema-core/src/storage/mod.rs          # Export blob module
4. noema-core/llm/src/api.rs              # ContentBlock variants
5. noema-core/src/storage/sqlite.rs       # New schema + refactor
6. noema-core/src/storage/traits.rs       # Trait updates
7. noema-core/src/storage/migrate.rs      # Migration logic
8. noema-ext/src/attachments.rs           # Blob-aware processing
9. noema-ui/src-tauri/src/commands/init.rs # Init updates
10. noema-ui/src-tauri/src/commands/chat.rs # Chat updates
11. noema-ui/src-tauri/src/types.rs       # Display updates
12. config/src/settings.rs                # Settings module
```

---

## Testing Strategy

1. **Unit Tests:**
   - BlobStore: store/retrieve/delete/hash verification
   - ContentBlock: serialization with refs
   - Schema: table creation, constraints

2. **Integration Tests:**
   - Full message flow with blob storage
   - Migration from old to new format
   - Backward compatibility with inline data

3. **Manual Testing:**
   - Migrate existing conversations
   - Verify images/audio display correctly
   - Check blob deduplication works

---

## Rollback Plan

1. Keep old `noema.db` as backup before migration
2. Schema version tracking allows identifying database state
3. ContentBlock supports both inline and ref formats during transition
4. Can revert PathManager to old paths if needed

---

## Files to Create

- `noema-core/src/storage/blob.rs`
- `noema-core/src/storage/migrate.rs`
- `config/src/settings.rs`

## Files to Modify

- `config/src/paths.rs`
- `config/src/lib.rs`
- `noema-core/src/storage/mod.rs`
- `noema-core/src/storage/sqlite.rs`
- `noema-core/src/storage/traits.rs`
- `noema-core/llm/src/api.rs`
- `noema-ext/src/attachments.rs`
- `noema-ui/src-tauri/src/commands/init.rs`
- `noema-ui/src-tauri/src/commands/chat.rs`
- `noema-ui/src-tauri/src/types.rs`
- `noema-core/src/mcp/config.rs`
