# Noema Storage Architecture

This document describes the storage layout, database schema, file locations, and configuration mechanisms used by Noema.

## Overview

Noema uses a combination of:
- **SQLite database** for conversations and messages
- **TOML files** for configuration (MCP servers)
- **Environment files** for API keys and provider settings
- **Binary files** for ML models (Whisper voice)

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
├── noema.db              # SQLite database (conversations, messages)
├── pending_oauth.json    # Temporary OAuth state (auto-deleted after completion)
└── models/
    └── ggml-base.en.bin  # Whisper voice model (~140MB, downloaded on-demand)

<config_dir>/
└── mcp.toml              # MCP server configuration

<cache_dir>/
└── (reserved for future use)

<logs_dir>/
└── (application logs)
```

---

## Database Schema

### File: `<data_dir>/noema.db`

SQLite database with WAL mode enabled for concurrent access.

### Tables

#### `conversations`

Stores conversation metadata.

| Column | Type | Constraints | Description |
|--------|------|-------------|-------------|
| `id` | TEXT | PRIMARY KEY | UUID v4 identifier |
| `name` | TEXT | nullable | User-defined conversation name |
| `created_at` | INTEGER | NOT NULL, DEFAULT now | Unix timestamp (seconds) |
| `updated_at` | INTEGER | NOT NULL, DEFAULT now | Unix timestamp (seconds) |

#### `messages`

Stores individual messages within conversations.

| Column | Type | Constraints | Description |
|--------|------|-------------|-------------|
| `id` | TEXT | PRIMARY KEY | UUID v4 identifier |
| `conversation_id` | TEXT | NOT NULL, FK → conversations | Parent conversation |
| `role` | TEXT | NOT NULL | `"user"`, `"assistant"`, or `"system"` |
| `payload` | JSON | NOT NULL | Serialized message content (see below) |
| `position` | INTEGER | NOT NULL | Ordering within conversation (0-indexed) |
| `created_at` | INTEGER | NOT NULL, DEFAULT now | Unix timestamp (seconds) |

**Index:** `idx_messages_conversation` on `(conversation_id, position)` for efficient retrieval.

### Message Payload Schema

The `payload` column contains JSON-serialized `ChatPayload` with the following structure:

```json
// Text message
{
  "Text": "Hello, world!"
}

// Image content
{
  "Image": {
    "mime_type": "image/png",
    "data": "<base64-encoded>"
  }
}

// Audio content
{
  "Audio": {
    "mime_type": "audio/wav",
    "data": "<base64-encoded>"
  }
}

// Tool call (assistant requesting tool execution)
{
  "ToolCall": {
    "id": "call_abc123",
    "name": "get_weather",
    "arguments": "{\"location\": \"NYC\"}"
  }
}

// Tool result (response to tool call)
{
  "ToolResult": {
    "tool_call_id": "call_abc123",
    "content": [
      {"type": "text", "text": "72°F, sunny"}
    ]
  }
}
```

### Transaction Semantics

Messages are written transactionally:

1. `session.begin()` → creates transaction with pending buffer
2. `tx.add(message)` → buffers message in memory
3. `session.commit(tx)` → writes all pending to SQLite atomically
4. On drop without commit → logs warning, discards pending

**Lazy creation:** Conversations are not persisted until the first message is committed.

---

## Configuration Files

### MCP Configuration

**File:** `<config_dir>/mcp.toml`

Defines MCP (Model Context Protocol) server connections.

```toml
[servers.filesystem]
name = "Local Filesystem"
url = "npx:-y--@anthropic-ai/mcp-filesystem-server/path/to/dir"

[servers.filesystem.auth]
type = "none"

[servers.github]
name = "GitHub"
url = "https://api.github.com/mcp"
use_well_known = true

[servers.github.auth]
type = "oauth"
client_id = "your-client-id"
client_secret = "your-client-secret"
# Populated after successful OAuth flow:
access_token = "gho_xxxxx"
refresh_token = "ghr_xxxxx"
expires_at = 1735689600

[servers.custom-api]
name = "Custom API"
url = "https://mcp.example.com"

[servers.custom-api.auth]
type = "token"
token = "sk-xxxxx"
```

**Auth types:**
- `none` - No authentication
- `token` - Bearer token authentication
- `oauth` - OAuth 2.0 flow (tokens stored after authorization)

### Environment Variables

**Files:** `~/.env` (global) and `./.env` (project-local)

Project-local values override global values.

```bash
# API Keys
CLAUDE_API_KEY=sk-ant-xxxxx
GEMINI_API_KEY=AIzaSy-xxxxx
OPENAI_API_KEY=sk-xxxxx

# Provider endpoint overrides
OLLAMA_BASE_URL=http://localhost:11434
ANTHROPIC_BASE_URL=https://custom-proxy.example.com
```

---

## Model Storage

### Whisper Voice Model

**File:** `<data_dir>/models/ggml-base.en.bin`

- **Format:** GGML binary (Whisper.cpp compatible)
- **Size:** ~140MB
- **Download:** On-demand via `download_voice_model` command
- **Source:** Hugging Face model hub

The model is downloaded only when voice features are first used.

---

## Temporary Files

### OAuth State

**File:** `<data_dir>/pending_oauth.json`

Stores pending OAuth authorization states for MCP server authentication.

```json
{
  "state_parameter_abc123": "server-id-xyz"
}
```

This file is:
- Created when initiating OAuth flow
- Read when OAuth callback is received
- Entry removed after successful authorization
- Safe to delete (will require re-authorization)

---

## Mobile Considerations

On Android and iOS, standard path detection may fail. The application accepts a custom data directory override:

```rust
PathManager::set_data_dir(app.path().app_data_dir()?);
```

When set, all other directories derive from this base:
- `config_dir` → `data_dir`
- `cache_dir` → `data_dir/cache`
- `logs_dir` → `data_dir/logs`

---

## In-Memory Caches

### Completion Cache

- **Purpose:** Cache command completion results
- **TTL:** 5 minutes
- **Scope:** Per-session, not persisted
- **Key format:** `"{input}:{partial}"`

### Session Message Cache

- **Purpose:** Avoid repeated database reads
- **Scope:** Per-session
- **Sync:** Updated on commit, reflects database state

---

## Initialization Sequence

On application startup:

1. **Storage Init**
   - Resolve `db_path()` based on platform
   - Create parent directories if needed
   - Open SQLite connection with WAL mode
   - Run schema creation (idempotent)

2. **Config Load**
   - Load `~/.env` (if exists)
   - Load `./.env` (if exists, overwrites globals)

3. **Session Create**
   - Generate new conversation UUID
   - (Conversation not written to DB until first message)

4. **MCP Load**
   - Load `mcp.toml` from config directory
   - Initialize server registry

5. **Engine Init**
   - Create chat engine with session and model

---

## Path Manager API

The `PathManager` struct provides all path resolution:

```rust
use config::PathManager;

// Base directories
PathManager::data_dir()    // → PathBuf
PathManager::config_dir()  // → PathBuf
PathManager::cache_dir()   // → PathBuf
PathManager::logs_dir()    // → PathBuf

// Specific files
PathManager::db_path()            // → data_dir/noema.db
PathManager::models_dir()         // → data_dir/models
PathManager::whisper_model_path() // → data_dir/models/ggml-base.en.bin

// Mobile override
PathManager::set_data_dir(path)   // Set custom base for mobile
```

---

## Backup and Migration

### Backing Up Data

Essential files to backup:
```
<data_dir>/noema.db       # All conversations and messages
<config_dir>/mcp.toml     # MCP server configuration
~/.env                    # API keys (if stored here)
```

### Database Migration

The schema is created on first run. Future migrations should:
1. Check schema version (not yet implemented)
2. Apply incremental migrations
3. Update version marker

Currently, schema changes require manual migration or database recreation.

---

## Security Considerations

1. **API Keys** - Stored in plaintext in `.env` files. Consider using system keychain for production.

2. **OAuth Tokens** - Stored in `mcp.toml`. File permissions should restrict access.

3. **Database** - Contains full conversation history. Encrypt at rest if handling sensitive data.

4. **Model Files** - Downloaded from external sources. Verify checksums when possible.
