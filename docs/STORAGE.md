# Unified Storage Architecture

**Version:** 1.0
**Target Architecture:** SQLite (Enhanced) + Content-Addressable Filesystem
**Goal:** Local-first storage with enterprise-grade capabilities.

---

## Overview

The unified storage uses:
- **SQLite database** with WAL mode for conversations, messages, threads, and metadata
- **Content-Addressable Storage (CAS)** for binary assets (images, audio, documents)
- **TOML configuration** for user preferences and MCP servers
- **Environment files** for secrets only (API keys, encryption key)
- **Vector extension** (`sqlite-vec`) for embedding-based search

All paths follow platform conventions (XDG on Linux, Application Support on macOS, AppData on Windows).

---

## Directory Structure

### Platform-Specific Base Directories

| Directory | macOS | Linux | Windows |
|-----------|-------|-------|---------|
| Data | `~/Library/Application Support/noema` | `~/.local/share/noema` | `%APPDATA%\noema` |
| Config | `~/Library/Preferences/noema` | `~/.config/noema` | `%APPDATA%\noema` |
| Cache | `~/Library/Caches/noema` | `~/.cache/noema` | `%APPDATA%\noema\cache` |
| Logs | `~/Library/Logs/Noema` | `~/.local/share/noema/logs` | `%APPDATA%\noema\logs` |

### Directory Layout

```
<data_dir>/
├── database/
│   ├── noema.db              # Main SQLite database (WAL mode)
│   └── noema.db-wal          # Write-Ahead Log
│
├── blob_storage/             # Content-Addressable Storage
│   ├── 7f/                   # Sharded by first 2 chars of SHA-256
│   │   └── 7f8a9b...png      # Actual file content
│   └── a1/
│       └── a1b2c3...pdf
│
├── config/
│   ├── settings.toml         # User preferences & MCP servers
│   └── .env                  # SECRETS ONLY (API Keys, Encryption Key)
│
└── models/
    └── ggml-base.en.bin      # Whisper voice model (~140MB)
```

---

## Database Technology Stack

| Component | Technology |
|-----------|------------|
| Engine | SQLite 3.45+ |
| Vector Extension | `sqlite-vec` |
| Mode | WAL (Write-Ahead Logging) |
| JSON Strategy | Native SQLite JSON functions (`json_extract`) |

---

## Database Schema

### File: `<data_dir>/database/noema.db`

### A. Identity & Auth

Single-tenant by default with "owner@local" user. Supports multi-tenant for enterprise deployments.

```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,                    -- UUID v4
    email TEXT UNIQUE NOT NULL,             -- Default: "owner@local"

    -- Encrypted API Keys (AES-256-GCM using ENCRYPTION_KEY)
    encrypted_anthropic_key TEXT,
    encrypted_openai_key TEXT,
    encrypted_gemini_key TEXT,

    -- OAuth persistence
    google_oauth_refresh_token TEXT,

    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

**Default User:** On first launch, create `owner@local` user automatically (bypass login for local mode).

### B. Conversations & Threads

Threads enable branching conversations from any message.

```sql
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,                    -- UUID v4
    user_id TEXT REFERENCES users(id),
    title TEXT,
    system_prompt TEXT,

    -- Semantic Search
    summary_text TEXT,
    summary_embedding BLOB,                 -- 1536-dim float vector (sqlite-vec)

    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE threads (
    id TEXT PRIMARY KEY,                    -- UUID v4
    conversation_id TEXT REFERENCES conversations(id) ON DELETE CASCADE,
    parent_message_id TEXT,                 -- Fork point (NULL for main thread)
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_threads_conversation ON threads(conversation_id);
```

### C. Messages

Messages contain structured content blocks with references to assets (no inline Base64).

```sql
CREATE TABLE messages (
    id TEXT PRIMARY KEY,                    -- UUID v4
    thread_id TEXT REFERENCES threads(id) ON DELETE CASCADE,
    role TEXT CHECK(role IN ('user', 'assistant', 'system')),

    -- Content: JSON Array of Content Blocks
    content_json TEXT NOT NULL,

    -- Search & Embeddings
    text_content TEXT,                      -- Extracted plain text for FTS
    embedding BLOB,                         -- 1536-dim vector (sqlite-vec)

    -- Metadata
    provider TEXT,                          -- 'anthropic', 'openai', 'google', 'ollama'
    model TEXT,
    tokens_used INTEGER,

    position INTEGER NOT NULL,              -- Ordering within thread
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_messages_thread ON messages(thread_id, position);
```

### D. Assets (Content-Addressable Storage)

Files are deduplicated by SHA-256 hash.

```sql
CREATE TABLE assets (
    id TEXT PRIMARY KEY,                    -- SHA-256 hash of content
    mime_type TEXT NOT NULL,
    original_filename TEXT,
    file_size_bytes INTEGER,

    metadata_json TEXT,                     -- Dimensions, duration, etc.

    local_path TEXT,                        -- Relative path in blob_storage/
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

---

## Content Block Schema

The `content_json` column in messages contains a JSON array of content blocks:

```json
// Text content
{"type": "text", "text": "Hello, world!"}

// Image reference (CAS)
{"type": "image", "asset_id": "7f8a9b...", "alt": "Screenshot"}

// Audio reference (CAS)
{"type": "audio", "asset_id": "a1b2c3...", "duration_ms": 5000}

// Document reference (CAS)
{"type": "document", "asset_id": "d4e5f6...", "filename": "report.pdf"}

// Tool call (assistant requesting execution)
{
  "type": "tool_call",
  "id": "call_abc123",
  "name": "get_weather",
  "arguments": {"location": "NYC"}
}

// Tool result
{
  "type": "tool_result",
  "tool_call_id": "call_abc123",
  "content": [{"type": "text", "text": "72°F, sunny"}]
}
```

---

## Content-Addressable Storage (CAS)

### How It Works

1. **Upload/Generate** → Calculate SHA-256 hash of file content
2. **Check** → Does `blob_storage/{hash[0:2]}/{hash}` exist?
3. **Store** → If not, write file to sharded directory
4. **Reference** → Store hash as `asset_id` in messages

### Directory Sharding

Files are sharded by the first 2 characters of their SHA-256 hash:

```
blob_storage/
├── 7f/
│   ├── 7f8a9bc4d5e6f7...png
│   └── 7fab12cd34ef56...jpg
├── a1/
│   └── a1b2c3d4e5f6a7...pdf
└── ff/
    └── ff00112233445566...wav
```

### Benefits

- **Deduplication** - Same file stored once regardless of how many times referenced
- **Integrity** - Hash verifies content hasn't changed
- **No Base64 bloat** - Binary files stay binary (saves ~33% space)
- **Streaming** - Files can be read/written incrementally

---

## Configuration Files

### Settings

**File:** `<data_dir>/config/settings.toml`

```toml
[storage]
data_dir = "~/.local/share/noema"  # Override default location

[ui]
theme = "system"                    # "light", "dark", "system"
font_size = 14

[models]
default_provider = "anthropic"
default_model = "claude-sonnet-4-20250514"

[mcp.servers.filesystem]
name = "Local Filesystem"
command = "npx"
args = ["-y", "@anthropic-ai/mcp-filesystem-server", "/path/to/dir"]

[mcp.servers.filesystem.auth]
type = "none"

[mcp.servers.github]
name = "GitHub"
url = "https://api.github.com/mcp"
use_well_known = true

[mcp.servers.github.auth]
type = "oauth"
client_id = "your-client-id"
```

### Secrets

**File:** `<data_dir>/config/.env`

```bash
# Master encryption key for API keys stored in DB
ENCRYPTION_KEY=base64-encoded-32-byte-key

# Alternative: Direct API keys (if not using encrypted DB storage)
ANTHROPIC_API_KEY=sk-ant-xxxxx
OPENAI_API_KEY=sk-xxxxx
GEMINI_API_KEY=AIzaSy-xxxxx

# Provider endpoint overrides
OLLAMA_BASE_URL=http://localhost:11434
```

**Security:** This file should have `600` permissions (owner read/write only).

---

## Model Storage

### Whisper Voice Model

**File:** `<data_dir>/models/ggml-base.en.bin`

| Property | Value |
|----------|-------|
| Format | GGML binary (Whisper.cpp) |
| Size | ~140MB |
| Download | On-demand via `download_voice_model` |
| Source | Hugging Face model hub |

---

## Encryption

### API Key Encryption

API keys stored in the `users` table are encrypted using AES-256-GCM:

1. Master key derived from `ENCRYPTION_KEY` in `.env`
2. Each key encrypted with unique nonce
3. Format: `nonce || ciphertext || tag` (base64 encoded)

### At-Rest Encryption (Optional)

For sensitive deployments, enable SQLite Encryption Extension (SEE) or SQLCipher.

---

## Vector Search

### sqlite-vec Integration

```sql
-- Create virtual table for vector search
CREATE VIRTUAL TABLE message_vectors USING vec0(
    embedding float[1536]
);

-- Insert embedding
INSERT INTO message_vectors(rowid, embedding)
VALUES (?, ?);

-- Similarity search (cosine distance)
SELECT messages.*, distance
FROM message_vectors
JOIN messages ON messages.rowid = message_vectors.rowid
WHERE embedding MATCH ?
ORDER BY distance
LIMIT 10;
```

### Embedding Dimensions

| Provider | Model | Dimensions |
|----------|-------|------------|
| OpenAI | text-embedding-3-small | 1536 |
| OpenAI | text-embedding-3-large | 3072 |
| Anthropic | (via Voyage) | 1024 |

---

## Migration Path

### From Noema (Base64 Storage)

1. **Extract Base64 blobs** from `messages.payload`
2. **Decode and hash** each blob
3. **Write to CAS** at `blob_storage/{hash[0:2]}/{hash}`
4. **Replace payload** with `{"type": "image", "asset_id": "..."}`
5. **Insert into assets table**

### From Episteme (PostgreSQL)

1. **Export** using `pg_dump` with `--format=plain`
2. **Transform** JSONB → TEXT, ARRAY → JSON
3. **Import** using SQLite `.read` or migration script
4. **Rebuild vectors** using sqlite-vec

---

## Initialization Sequence

```
1. Resolve data_dir (platform-specific or override)
2. Create directory structure if missing
3. Open SQLite with WAL mode
4. Run schema migrations (idempotent)
5. Ensure "owner@local" user exists
6. Load settings.toml
7. Load .env secrets
8. Initialize MCP registry
9. Create default thread if none exists
```

---

## Path Manager API

```rust
use config::PathManager;

// Base directories
PathManager::data_dir()     // → ~/.local/share/noema
PathManager::config_dir()   // → ~/.config/noema (or data_dir/config)
PathManager::cache_dir()    // → ~/.cache/noema
PathManager::logs_dir()     // → data_dir/logs

// Database
PathManager::db_dir()       // → data_dir/database
PathManager::db_path()      // → data_dir/database/noema.db

// Content-Addressable Storage
PathManager::blob_dir()     // → data_dir/blob_storage
PathManager::blob_path(hash: &str) // → data_dir/blob_storage/{hash[0:2]}/{hash}

// Models
PathManager::models_dir()   // → data_dir/models
PathManager::whisper_model_path() // → data_dir/models/ggml-base.en.bin

// Config files
PathManager::settings_path() // → data_dir/config/settings.toml
PathManager::env_path()      // → data_dir/config/.env
```

---

## Backup & Restore

### Essential Files

```bash
# Full backup
tar -czf noema-backup.tar.gz \
  ~/.local/share/noema/database/ \
  ~/.local/share/noema/blob_storage/ \
  ~/.local/share/noema/config/

# Restore
tar -xzf noema-backup.tar.gz -C ~/
```

### Database-Only Backup

```bash
sqlite3 ~/.local/share/noema/database/noema.db ".backup backup.db"
```

---

## Security Considerations

| Asset | Protection |
|-------|------------|
| API Keys | AES-256-GCM encryption in DB |
| `.env` file | File permissions (600) |
| OAuth tokens | Encrypted in settings.toml |
| Conversation data | Optional SQLCipher encryption |
| Blob storage | Filesystem permissions |

### Recommendations

1. **Never commit** `.env` or `settings.toml` with secrets
2. **Use keychain** integration for production deployments
3. **Enable disk encryption** on the host system
4. **Audit blob_storage** - files are named by hash, not original filename
