# Unified Noema Architecture: Rust Everywhere

**Status**: Planning
**Created**: 2026-01-05
**Goal**: Consolidate Noema (Rust/Tauri) and Episteme (Python/FastAPI) into a single Rust-based platform supporting desktop, mobile, and web.

---

## Executive Summary

This document outlines a plan to unify two existing codebases:

- **Noema** (`~/projects/simply/noema`): Rust/Tauri desktop app with React frontend
- **Episteme** (`~/projects/simply/episteme`): Python/FastAPI cloud app with React frontend

**Decision**: Deprecate Episteme and build a Rust web server (`noema-web`) that shares `noema-core` with Tauri. This achieves ~95% code sharing across all platforms.

### Why Rust Everywhere?

| Approach | Code Sharing | Maintenance | Web Access |
|----------|--------------|-------------|------------|
| Keep both (Federated) | ~50% (frontend only) | Two backends (Rust + Python) | ✅ Episteme |
| Unified + WASM | ~90% | One backend | ⚠️ Experimental |
| **Rust Everywhere** | **~95%** | **One backend** | ✅ Axum server |

The "Rust Everywhere" approach maximizes code sharing while providing production-ready web access via Axum.

---

## Current State Analysis

### Noema (Rust/Tauri)

**Location**: `/Users/simplychris/projects/simply/noema`

**Stack**:
- Backend: Rust workspace with 8 crates
- Frontend: React 19 + TypeScript + Vite + Tailwind
- Desktop: Tauri 2.0
- Storage: SQLite + Content-Addressable Blob Storage
- LLM: Claude, OpenAI, Gemini, Ollama, Mistral

**Strengths**:
- Local-first architecture with offline support
- Trait-based abstractions (`SessionStore`, `ChatModel`, `Agent`)
- 70% mobile-ready (Tauri mobile support partially implemented)
- Excellent storage design (see `docs/STORAGE.md`)

**Key Crates**:
```
noema/
├── noema-core/          # Core agent framework, storage, chat engine
│   └── llm/             # LLM provider abstraction
├── noema-ui/            # React frontend + Tauri bridge
│   └── src-tauri/       # Tauri commands
├── noema-audio/         # Voice capture + Whisper transcription
├── config/              # Path management, encryption, settings
├── noema-ext/           # Extensions (PDF extraction)
├── noema-mcp-gdocs/     # Google Docs MCP server
└── commands/            # CLI command definitions
```

### Episteme (Python/FastAPI)

**Location**: `/Users/simplychris/projects/simply/episteme`

**Stack**:
- Backend: Python 3.12+ with FastAPI
- Frontend: React 18+ TypeScript + Vite
- Database: PostgreSQL 15+ with pgvector (or SQLite for local)
- Auth: JWT + OAuth 2.0

**Strengths**:
- Multi-user ready with JWT authentication
- PostgreSQL with pgvector for semantic search
- WebSocket streaming for real-time responses
- Full REST API (55+ endpoints)
- Supabase integration documented

**Features to Port**:
1. Multi-user authentication (JWT)
2. pgvector semantic search
3. WebSocket streaming protocol
4. User management endpoints

---

## Target Architecture

```
                        ┌─────────────────────────────┐
                        │      Shared React UI        │
                        │     noema-ui/src/           │
                        └──────────────┬──────────────┘
                                       │
                    ┌──────────────────┼──────────────────┐
                    │                  │                  │
           ┌────────▼────────┐ ┌───────▼───────┐ ┌───────▼───────┐
           │   Tauri         │ │    Tauri      │ │   noema-web   │
           │   Desktop       │ │    Mobile     │ │   (Axum)      │
           │ macOS/Win/Linux │ │  iOS/Android  │ │               │
           └────────┬────────┘ └───────┬───────┘ └───────┬───────┘
                    │                  │                 │
                    │    Tauri IPC     │                 │ HTTP/WS
                    │                  │                 │
                    └──────────────────┼─────────────────┘
                                       │
                            ┌──────────▼──────────┐
                            │     noema-core      │
                            │                     │
                            │  ┌───────────────┐  │
                            │  │  ChatEngine   │  │
                            │  └───────────────┘  │
                            │  ┌───────────────┐  │
                            │  │ LLM Providers │  │
                            │  └───────────────┘  │
                            │  ┌───────────────┐  │
                            │  │ MCP Client    │  │
                            │  └───────────────┘  │
                            │  ┌───────────────┐  │
                            │  │ SqliteStore   │  │
                            │  └───────────────┘  │
                            └──────────┬──────────┘
                                       │
                            ┌──────────▼──────────┐
                            │    noema-sync       │
                            │  (optional cloud)   │
                            │                     │
                            │  - Event sourcing   │
                            │  - PostgreSQL       │
                            │  - Conflict res.    │
                            └─────────────────────┘
```

### Platform Matrix

| Platform | Runtime | Backend | Storage | Auth |
|----------|---------|---------|---------|------|
| Desktop | Tauri | noema-core | Local SQLite | None (single-user) |
| Mobile | Tauri | noema-core | Local SQLite | None (single-user) |
| Web | Axum | noema-core | SQLite or PostgreSQL | JWT (multi-user) |
| Web + Sync | Axum | noema-core + noema-sync | PostgreSQL | JWT + OAuth |

---

## Implementation Phases

### Phase 1: Create noema-web Crate

**Goal**: Axum-based web server that wraps `noema-core` and serves the React frontend.

#### 1.1 Project Setup

Create new crate in workspace:

```toml
# Cargo.toml (workspace root)
[workspace]
members = [
    # ... existing members
    "noema-web",
]
```

```toml
# noema-web/Cargo.toml
[package]
name = "noema-web"
version = "0.1.0"
edition = "2021"

[dependencies]
noema-core = { path = "../noema-core" }
config = { path = "../config" }

# Web framework
axum = { version = "0.7", features = ["ws", "multipart"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["fs", "cors", "trace"] }

# Async runtime
tokio = { version = "1", features = ["full"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Auth
jsonwebtoken = "9"
argon2 = "0.5"

# Utilities
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v4"] }
```

#### 1.2 Directory Structure

```
noema-web/
├── Cargo.toml
├── src/
│   ├── main.rs              # Server entrypoint
│   ├── lib.rs               # Library exports
│   ├── config.rs            # Server configuration
│   ├── error.rs             # Error types
│   ├── auth/
│   │   ├── mod.rs
│   │   ├── jwt.rs           # JWT token handling
│   │   ├── middleware.rs    # Auth middleware
│   │   └── password.rs      # Password hashing
│   ├── routes/
│   │   ├── mod.rs           # Router setup
│   │   ├── conversations.rs # Conversation CRUD
│   │   ├── messages.rs      # Message handling
│   │   ├── models.rs        # Model listing
│   │   ├── documents.rs     # Document management
│   │   ├── users.rs         # User management
│   │   └── health.rs        # Health check
│   ├── ws/
│   │   ├── mod.rs
│   │   └── handler.rs       # WebSocket for streaming
│   └── state.rs             # Application state
```

#### 1.3 Core Server Implementation

```rust
// noema-web/src/main.rs
use axum::{Router, routing::get};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::init();

    let config = Config::from_env();
    let state = AppState::new(&config).await?;

    let app = Router::new()
        // API routes
        .nest("/api", routes::api_router())
        // WebSocket
        .route("/api/ws", get(ws::handler))
        // Static files (React build)
        .fallback_service(ServeDir::new(&config.static_dir))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("Server running on {}", addr);

    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        app
    ).await?;
}
```

#### 1.4 API Routes Mapping

Map Tauri commands to HTTP endpoints:

| Tauri Command | HTTP Endpoint | Method |
|---------------|---------------|--------|
| `list_conversations` | `/api/conversations` | GET |
| `create_conversation` | `/api/conversations` | POST |
| `get_conversation` | `/api/conversations/:id` | GET |
| `delete_conversation` | `/api/conversations/:id` | DELETE |
| `rename_conversation` | `/api/conversations/:id` | PATCH |
| `send_message` | `/api/conversations/:id/messages` | POST |
| `get_messages` | `/api/conversations/:id/messages` | GET |
| `list_models` | `/api/models` | GET |
| `set_model` | `/api/models/current` | PUT |
| `list_documents` | `/api/documents` | GET |
| `import_document` | `/api/documents/import` | POST |
| - | `/api/ws` | WebSocket |

#### 1.5 Example Route Implementation

```rust
// noema-web/src/routes/conversations.rs
use axum::{
    extract::{Path, State},
    Json,
};
use noema_core::storage::SqliteStore;

pub async fn list_conversations(
    State(state): State<AppState>,
    user: AuthenticatedUser,  // From auth middleware
) -> Result<Json<Vec<ConversationInfo>>, ApiError> {
    let store = state.store.lock().await;
    let conversations = store.list_conversations(&user.id)?;
    Ok(Json(conversations))
}

pub async fn create_conversation(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<ConversationInfo>, ApiError> {
    let store = state.store.lock().await;
    let session = store.create_conversation(&user.id)?;

    if let Some(title) = req.title {
        store.rename_conversation(&session.conversation_id, &title)?;
    }

    let info = store.get_conversation_info(&session.conversation_id)?;
    Ok(Json(info))
}
```

#### 1.6 WebSocket Streaming

```rust
// noema-web/src/ws/handler.rs
use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade}, State},
    response::Response,
};

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, user))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, user: AuthenticatedUser) {
    while let Some(Ok(msg)) = socket.recv().await {
        match serde_json::from_str::<WsMessage>(&msg.to_text().unwrap_or_default()) {
            Ok(WsMessage::SendMessage { conversation_id, content }) => {
                // Stream response back via WebSocket
                let mut rx = state.engine.send_message_streaming(
                    &conversation_id,
                    content,
                ).await?;

                while let Some(delta) = rx.recv().await {
                    socket.send(Message::Text(
                        serde_json::to_string(&WsResponse::Delta(delta))?
                    )).await?;
                }
            }
            // ... handle other message types
        }
    }
}
```

---

### Phase 2: Unify React Frontend

**Goal**: Single React codebase that works with both Tauri IPC and HTTP/WebSocket.

#### 2.1 Adapter Interface

```typescript
// noema-ui/src/api/types.ts
export interface ConversationInfo {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: ContentBlock[];
  created_at: string;
}

export interface ApiAdapter {
  // Conversations
  listConversations(): Promise<ConversationInfo[]>;
  createConversation(title?: string): Promise<ConversationInfo>;
  getConversation(id: string): Promise<ConversationInfo>;
  deleteConversation(id: string): Promise<void>;
  renameConversation(id: string, title: string): Promise<void>;

  // Messages
  getMessages(conversationId: string, threadId?: string): Promise<Message[]>;
  sendMessage(conversationId: string, content: ContentBlock[]): Promise<void>;

  // Streaming
  onStreamingDelta(callback: (delta: StreamDelta) => void): () => void;
  onMessageComplete(callback: (message: Message) => void): () => void;

  // Models
  listModels(): Promise<ModelInfo[]>;
  setModel(provider: string, modelId: string): Promise<void>;

  // Documents
  listDocuments(): Promise<DocumentInfo[]>;
  importDocument(request: ImportRequest): Promise<DocumentInfo>;
}
```

#### 2.2 Tauri Adapter

```typescript
// noema-ui/src/api/tauri-adapter.ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { ApiAdapter, ConversationInfo, Message } from './types';

export function createTauriAdapter(): ApiAdapter {
  return {
    async listConversations() {
      return invoke<ConversationInfo[]>('list_conversations');
    },

    async createConversation(title?: string) {
      return invoke<ConversationInfo>('create_conversation', { title });
    },

    async sendMessage(conversationId: string, content: ContentBlock[]) {
      return invoke('send_message', { conversationId, content });
    },

    onStreamingDelta(callback) {
      const unlisten = listen<StreamDelta>('streaming-delta', (event) => {
        callback(event.payload);
      });
      return () => { unlisten.then(fn => fn()); };
    },

    // ... rest of implementation
  };
}
```

#### 2.3 HTTP Adapter

```typescript
// noema-ui/src/api/http-adapter.ts
import type { ApiAdapter, ConversationInfo } from './types';

export function createHttpAdapter(baseUrl: string = ''): ApiAdapter {
  let ws: WebSocket | null = null;
  const listeners = new Map<string, Set<(data: any) => void>>();

  // Initialize WebSocket
  function connectWs() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    ws = new WebSocket(`${protocol}//${window.location.host}/api/ws`);

    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      listeners.get(msg.type)?.forEach(cb => cb(msg.data));
    };
  }

  return {
    async listConversations() {
      const res = await fetch(`${baseUrl}/api/conversations`);
      return res.json();
    },

    async createConversation(title?: string) {
      const res = await fetch(`${baseUrl}/api/conversations`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title }),
      });
      return res.json();
    },

    async sendMessage(conversationId: string, content: ContentBlock[]) {
      ws?.send(JSON.stringify({
        type: 'send_message',
        conversation_id: conversationId,
        content,
      }));
    },

    onStreamingDelta(callback) {
      if (!listeners.has('delta')) listeners.set('delta', new Set());
      listeners.get('delta')!.add(callback);
      return () => listeners.get('delta')?.delete(callback);
    },

    // ... rest of implementation
  };
}
```

#### 2.4 Auto-Detection

```typescript
// noema-ui/src/api/index.ts
import { createTauriAdapter } from './tauri-adapter';
import { createHttpAdapter } from './http-adapter';
import type { ApiAdapter } from './types';

function detectEnvironment(): 'tauri' | 'web' {
  // @ts-ignore - Tauri injects this global
  return typeof window.__TAURI__ !== 'undefined' ? 'tauri' : 'web';
}

export const api: ApiAdapter = detectEnvironment() === 'tauri'
  ? createTauriAdapter()
  : createHttpAdapter();

// Re-export types
export * from './types';
```

#### 2.5 Component Migration

Before (Tauri-specific):
```typescript
// OLD: Direct invoke calls
import { invoke } from '@tauri-apps/api/core';

function ConversationList() {
  const [conversations, setConversations] = useState([]);

  useEffect(() => {
    invoke('list_conversations').then(setConversations);
  }, []);
}
```

After (Adapter pattern):
```typescript
// NEW: Use adapter
import { api } from '@/api';

function ConversationList() {
  const [conversations, setConversations] = useState([]);

  useEffect(() => {
    api.listConversations().then(setConversations);
  }, []);
}
```

---

### Phase 3: Multi-User Support

**Goal**: Add user scoping to `noema-core` for web deployment.

#### 3.1 User Context

```rust
// noema-core/src/context.rs
pub struct UserContext {
    pub user_id: String,
    pub email: String,
}

impl UserContext {
    pub fn default_local() -> Self {
        Self {
            user_id: "local-user".into(),
            email: "human@noema".into(),
        }
    }
}
```

#### 3.2 Storage Updates

Update `SqliteStore` methods to require user context:

```rust
// noema-core/src/storage/sqlite.rs

impl SqliteStore {
    // Before: pub fn list_conversations(&self) -> Result<Vec<ConversationInfo>>
    // After:
    pub fn list_conversations(&self, user_id: &str) -> Result<Vec<ConversationInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at
             FROM conversations
             WHERE user_id = ?
             ORDER BY updated_at DESC"
        )?;
        // ...
    }

    pub fn create_conversation(&self, user_id: &str) -> Result<SqliteSession> {
        // Insert with user_id
    }
}
```

#### 3.3 Backward Compatibility

For desktop (single-user), use a constant user ID:

```rust
// noema-ui/src-tauri/src/commands/conversations.rs
const LOCAL_USER_ID: &str = "local-user";

#[tauri::command]
pub async fn list_conversations(state: State<'_, AppState>) -> Result<Vec<ConversationInfo>, String> {
    let store = state.store.lock().await;
    store.list_conversations(LOCAL_USER_ID)
        .map_err(|e| e.to_string())
}
```

---

### Phase 4: Cloud Sync (Optional)

**Goal**: Enable cross-device synchronization for users who opt in.

#### 4.1 Sync Event Schema

```rust
// noema-sync/src/events.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEvent {
    pub id: Uuid,
    pub timestamp: i64,           // Unix millis
    pub device_id: String,
    pub user_id: String,
    pub event_type: SyncEventType,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub payload: serde_json::Value,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncEventType {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityType {
    Conversation,
    Thread,
    Message,
    Document,
    Settings,
}
```

#### 4.2 Sync Provider Trait

```rust
// noema-sync/src/traits.rs
use async_trait::async_trait;

#[async_trait]
pub trait SyncProvider: Send + Sync {
    /// Push local events to remote
    async fn push(&self, events: &[SyncEvent]) -> Result<PushResult>;

    /// Pull events since last sync
    async fn pull(&self, since: Option<i64>) -> Result<Vec<SyncEvent>>;

    /// Get current sync status
    fn status(&self) -> SyncStatus;
}

pub struct PushResult {
    pub applied: Vec<Uuid>,
    pub conflicts: Vec<SyncConflict>,
}

pub struct SyncConflict {
    pub event_id: Uuid,
    pub local_version: i64,
    pub remote_version: i64,
    pub resolution: ConflictResolution,
}

pub enum ConflictResolution {
    LocalWins,
    RemoteWins,
    Manual(serde_json::Value),
}
```

#### 4.3 PostgreSQL Backend

```rust
// noema-sync/src/postgres.rs
use sqlx::{PgPool, postgres::PgPoolOptions};

pub struct PostgresSyncProvider {
    pool: PgPool,
    user_id: String,
    device_id: String,
}

impl PostgresSyncProvider {
    pub async fn new(database_url: &str, user_id: &str, device_id: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        Ok(Self {
            pool,
            user_id: user_id.into(),
            device_id: device_id.into(),
        })
    }
}

#[async_trait]
impl SyncProvider for PostgresSyncProvider {
    async fn push(&self, events: &[SyncEvent]) -> Result<PushResult> {
        let mut tx = self.pool.begin().await?;

        for event in events {
            sqlx::query(
                "INSERT INTO sync_events (id, timestamp, device_id, user_id, event_type, entity_type, entity_id, payload, version)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT (entity_id, version) DO NOTHING"
            )
            .bind(&event.id)
            .bind(event.timestamp)
            .bind(&event.device_id)
            .bind(&event.user_id)
            .bind(&event.event_type)
            .bind(&event.entity_type)
            .bind(&event.entity_id)
            .bind(&event.payload)
            .bind(event.version)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(PushResult { applied: events.iter().map(|e| e.id).collect(), conflicts: vec![] })
    }

    async fn pull(&self, since: Option<i64>) -> Result<Vec<SyncEvent>> {
        let since = since.unwrap_or(0);

        let events = sqlx::query_as::<_, SyncEvent>(
            "SELECT * FROM sync_events
             WHERE user_id = $1 AND timestamp > $2 AND device_id != $3
             ORDER BY timestamp ASC"
        )
        .bind(&self.user_id)
        .bind(since)
        .bind(&self.device_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    fn status(&self) -> SyncStatus {
        SyncStatus::Connected
    }
}
```

#### 4.4 Local Event Queue

Add to SQLite schema for tracking pending sync:

```sql
-- Add to noema.db
CREATE TABLE IF NOT EXISTS sync_queue (
    id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    version INTEGER NOT NULL,
    synced_at INTEGER,
    retry_count INTEGER DEFAULT 0
);

CREATE INDEX idx_sync_queue_pending ON sync_queue(synced_at) WHERE synced_at IS NULL;
```

---

### Phase 5: Mobile Deployment

**Goal**: Ship Noema on iOS and Android via Tauri 2.0.

#### 5.1 Generate Platform Projects

```bash
# Initialize iOS
cargo tauri ios init

# Initialize Android
cargo tauri android init
```

#### 5.2 Update Tauri Config

```json
// noema-ui/src-tauri/tauri.conf.json
{
  "bundle": {
    "android": {
      "minSdkVersion": 24
    },
    "iOS": {
      "developmentTeam": "YOUR_TEAM_ID",
      "minimumSystemVersion": "13.0"
    }
  }
}
```

#### 5.3 Platform-Specific Permissions

**Android** (`src-tauri/gen/android/app/src/main/AndroidManifest.xml`):
```xml
<uses-permission android:name="android.permission.INTERNET" />
<uses-permission android:name="android.permission.RECORD_AUDIO" />
<uses-permission android:name="android.permission.READ_EXTERNAL_STORAGE" />
```

**iOS** (`src-tauri/gen/ios/*/Info.plist`):
```xml
<key>NSMicrophoneUsageDescription</key>
<string>Noema needs microphone access for voice input</string>
```

#### 5.4 Mobile Path Handling

Already implemented in `noema-ui/src-tauri/src/lib.rs`:

```rust
#[cfg(any(target_os = "android", target_os = "ios"))]
{
    use tauri::Manager;
    if let Ok(dir) = app.path().app_data_dir() {
        config::PathManager::set_data_dir(dir);
    }
}
```

#### 5.5 Build Commands

```bash
# Development
cargo tauri ios dev
cargo tauri android dev

# Release
cargo tauri ios build --release
cargo tauri android build --release
```

---

### Phase 6: Episteme Migration & Deprecation

**Goal**: Port valuable features, migrate users, archive Python codebase.

#### 6.1 Feature Audit

| Episteme Feature | Noema Status | Action |
|------------------|--------------|--------|
| Multi-user auth | Not present | Port to noema-web |
| PostgreSQL storage | Not present | Add to noema-sync |
| pgvector search | Not present | Add to noema-sync |
| WebSocket streaming | Not present | Add to noema-web |
| Google Drive integration | ✅ Present | None |
| Document system | ✅ Present | Verify parity |
| MCP tool handling | ✅ Present | None |
| Conversation branching | ✅ Present | None |

#### 6.2 Data Migration Script

```python
# scripts/migrate_episteme.py
"""Export Episteme data to JSON for import into Noema"""

import json
from episteme.backend.storage import get_session
from episteme.backend.models import Conversation, Message, Document

def export_user_data(user_id: str, output_path: str):
    session = get_session()

    data = {
        "version": "1.0",
        "user_id": user_id,
        "conversations": [],
        "documents": [],
    }

    # Export conversations
    for conv in session.query(Conversation).filter_by(user_id=user_id).all():
        conv_data = {
            "id": conv.id,
            "title": conv.title,
            "created_at": conv.created_at.isoformat(),
            "threads": [],
        }
        for thread in conv.threads:
            thread_data = {
                "id": thread.id,
                "messages": [
                    {
                        "id": msg.id,
                        "role": msg.role,
                        "content": msg.content,
                        "created_at": msg.created_at.isoformat(),
                    }
                    for msg in thread.messages
                ]
            }
            conv_data["threads"].append(thread_data)
        data["conversations"].append(conv_data)

    # Export documents
    for doc in session.query(Document).filter_by(user_id=user_id).all():
        data["documents"].append({
            "id": doc.id,
            "title": doc.title,
            "source": doc.source,
            "tabs": [
                {"title": tab.title, "content": tab.content_markdown}
                for tab in doc.tabs
            ]
        })

    with open(output_path, 'w') as f:
        json.dump(data, f, indent=2)

if __name__ == "__main__":
    import sys
    export_user_data(sys.argv[1], sys.argv[2])
```

#### 6.3 Import Command in Noema

```rust
// noema-web/src/routes/import.rs
use axum::{extract::Multipart, Json};

pub async fn import_episteme_data(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    mut multipart: Multipart,
) -> Result<Json<ImportResult>, ApiError> {
    while let Some(field) = multipart.next_field().await? {
        let data = field.bytes().await?;
        let export: EpistemeExport = serde_json::from_slice(&data)?;

        let store = state.store.lock().await;

        for conv in export.conversations {
            // Import conversation
            let session = store.create_conversation(&user.id)?;
            store.rename_conversation(&session.conversation_id, &conv.title)?;

            // Import messages
            for thread in conv.threads {
                for msg in thread.messages {
                    store.add_message(&session.conversation_id, &msg)?;
                }
            }
        }

        for doc in export.documents {
            store.create_document(&user.id, &doc.title, doc.source, None)?;
            // Import tabs...
        }
    }

    Ok(Json(ImportResult { success: true }))
}
```

---

## Deployment Guide

### Desktop (Current)

No changes required. Continue using:
```bash
cargo tauri build
```

### Mobile

```bash
# iOS (requires macOS + Xcode)
cargo tauri ios build --release

# Android (requires Android SDK + NDK)
cargo tauri android build --release
```

### Web Server

#### Option A: Binary

```bash
cd noema-web
cargo build --release

# Run
./target/release/noema-web \
  --port 8080 \
  --static-dir ../noema-ui/dist \
  --database-url sqlite:///data/noema.db
```

#### Option B: Docker

```dockerfile
# Dockerfile
FROM rust:1.75-bookworm as builder
WORKDIR /app
COPY . .
RUN cargo build --release -p noema-web
RUN cd noema-ui && npm ci && npm run build

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/noema-web /usr/local/bin/
COPY --from=builder /app/noema-ui/dist /app/dist
ENV STATIC_DIR=/app/dist
EXPOSE 8080
CMD ["noema-web"]
```

```bash
docker build -t noema-web .
docker run -p 8080:8080 -v noema-data:/data noema-web
```

#### Option C: Fly.io

```toml
# fly.toml
app = "noema-web"
primary_region = "sjc"

[build]
  dockerfile = "Dockerfile"

[env]
  PORT = "8080"
  STATIC_DIR = "/app/dist"

[http_service]
  internal_port = 8080
  force_https = true

[[mounts]]
  source = "noema_data"
  destination = "/data"
```

```bash
fly launch
fly deploy
```

#### Option D: Railway

```json
// railway.json
{
  "build": {
    "builder": "DOCKERFILE"
  },
  "deploy": {
    "startCommand": "noema-web",
    "healthcheckPath": "/api/health"
  }
}
```

---

## File Reference

### Files to Create

| Path | Purpose |
|------|---------|
| `noema-web/` | New Axum web server crate |
| `noema-sync/` | Optional sync crate |
| `noema-ui/src/api/types.ts` | Shared TypeScript types |
| `noema-ui/src/api/adapter.ts` | Adapter interface |
| `noema-ui/src/api/tauri-adapter.ts` | Tauri implementation |
| `noema-ui/src/api/http-adapter.ts` | HTTP implementation |
| `scripts/migrate_episteme.py` | Migration script |

### Files to Modify

| Path | Changes |
|------|---------|
| `Cargo.toml` | Add workspace members |
| `noema-core/src/storage/sqlite.rs` | Add user_id scoping |
| `noema-core/src/storage/traits.rs` | Update trait signatures |
| `noema-ui/src-tauri/src/commands/*.rs` | Use constant user ID |
| `noema-ui/src/*.tsx` | Switch from `invoke()` to `api.*()` |
| `noema-ui/src-tauri/tauri.conf.json` | Add iOS config |

### Files to Deprecate

| Path | Action |
|------|--------|
| `~/projects/simply/episteme/` | Archive after migration |

---

## Success Criteria

- [ ] `noema-web` serves React app and handles all API calls
- [ ] Same React code runs in Tauri (desktop/mobile) and browser
- [ ] Multi-user authentication works in web mode
- [ ] Desktop app functionality unchanged
- [ ] Mobile builds successfully on iOS and Android
- [ ] Episteme users can migrate data to new system
- [ ] (Optional) Cross-device sync works

---

## Risk Mitigation

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Axum complexity | Low | Medium | Well-documented, similar to other Rust web frameworks |
| Mobile Tauri bugs | Medium | Medium | Start with Android (more mature), keep PWA fallback |
| Feature parity gap | Medium | High | Audit Episteme features before deprecation |
| Migration data loss | Low | High | Test migration thoroughly, keep Episteme running during transition |
| Sync conflicts | Medium | Medium | Start with simple last-write-wins, add manual resolution later |

---

## Timeline Estimate

| Phase | Effort | Dependencies |
|-------|--------|--------------|
| 1. noema-web | 3-4 weeks | None |
| 2. Unified frontend | 2-3 weeks | Phase 1 |
| 3. Multi-user | 1-2 weeks | Phase 1 |
| 4. Cloud sync | 3-4 weeks | Phase 3 |
| 5. Mobile | 2-3 weeks | Phase 2 |
| 6. Migration | 1-2 weeks | Phase 1-3 |

**Total**: ~12-18 weeks (phases can partially overlap)
