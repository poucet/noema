# Refactoring Plan: Shared Backend Logic

## Objective
Extract the business logic currently embedded in `noema-desktop` (Tauri) into a shared library crate `noema-backend`. This will enable code reuse between the desktop application and a future web application (`noema-web`), while accommodating their specific differences (storage, auth, audio).

## 1. Architecture Overview

We will introduce a layered architecture:

*   **`noema-core`**: (Existing) Core domain logic, LLM clients, basic storage traits (`SessionStore`).
*   **`noema-backend`**: (New) Service layer. Contains high-level business logic, defines ports (traits) for infrastructure dependencies, and manages application state.
*   **`noema-desktop`**: (Existing) Thin Tauri wrapper. Implements "local" adapters (SQLite, FileSystem BlobStore, CPAL Audio).
*   **`noema-web`**: (Future) Thin Web wrapper. Implements "cloud" adapters (Postgres, S3 BlobStore, Web APIs).

### Layers

1.  **API / Transport Layer** (in wrappers):
    *   Tauri Commands (Desktop)
    *   REST/gRPC Endpoints (Web)
2.  **Service Layer** (in `noema-backend`):
    *   `ChatService`: Orchestrates message sending, tool use, and parallel model execution.
    *   `ConversationService`: Manages conversation/thread lifecycles.
    *   `UserService`: Manages user profiles and settings.
    *   `FileService`: Manages attachments and documents.
3.  **Port / Adapter Layer** (Traits in `noema-backend`, Impls in wrappers):
    *   `StorageProvider`: Abstract interface for persistence (Conversations, Threads, Messages).
    *   `BlobStorageProvider`: Abstract interface for asset storage.
    *   `AudioProvider`: Abstract interface for audio I/O (if shared logic exists).

## 2. Key Abstractions (`noema-backend/src/ports/`)

We need to define traits to decouple the service layer from specific implementations.

### Storage Provider
The current `SqliteStore` in `noema-core` is concrete. We should define a trait that covers the "Manager" capabilities.

```rust
#[async_trait]
pub trait StorageProvider: Send + Sync {
    // Conversation Management
    async fn list_conversations(&self, user_id: &str) -> Result<Vec<ConversationInfo>>;
    async fn create_conversation(&self, user_id: &str) -> Result<String>; // Returns ID
    async fn delete_conversation(&self, id: &str) -> Result<()>;
    
    // Session Access
    // Returns a SessionStore trait object (from noema-core)
    async fn get_session(&self, conversation_id: &str) -> Result<Box<dyn SessionStore>>;
    
    // Thread/Fork Management
    async fn list_threads(&self, conversation_id: &str) -> Result<Vec<ThreadInfo>>;
    // ... other methods currently in SqliteStore
}
```

### Blob Storage Provider
Abstract the file system dependency.

```rust
#[async_trait]
pub trait BlobStorageProvider: Send + Sync {
    async fn put(&self, id: &str, data: &[u8]) -> Result<()>;
    async fn get(&self, id: &str) -> Result<Vec<u8>>;
    async fn exists(&self, id: &str) -> Result<bool>;
}
```

## 3. Service Layer (`noema-backend/src/services/`)

These services will contain the logic extracted from `noema-desktop/commands`.

### `ChatService`
*   **Dependencies**: `Box<dyn StorageProvider>`, `Box<dyn BlobStorageProvider>`, `McpRegistry`.
*   **Methods**:
    *   `send_message(user_id, conv_id, payload)`
    *   `send_parallel_message(...)`
    *   `regenerate(...)`
*   **Logic**: Handles the event loop (currently in `commands/chat.rs`), coordinates with LLM, updates storage.

### `ConversationService`
*   **Dependencies**: `Box<dyn StorageProvider>`.
*   **Methods**:
    *   `list(user_id)`
    *   `create(user_id)`
    *   `fork(user_id, span_id)`
    *   `switch_thread(...)`

## 4. Addressing Specific Requirements

### 1. Different Blob Storage
*   **Desktop**: Implement `BlobStorageProvider` using `noema_core::storage::BlobStore` (Local FS).
*   **Web**: Implement `BlobStorageProvider` using AWS S3 SDK (or similar).

### 2. Per-User API Keys
*   **Desktop**: Single-user. Keys stored in local settings/keychain. `UserService` can mock authentication or use a default "Owner" user.
*   **Web**: Multi-user. Keys stored in DB linked to `user_id`. `UserService` resolves keys from the DB at runtime.
*   **Implementation**: Pass `Context` or `AuthToken` to service methods. Service looks up keys based on the user context.

### 3. Audio Processing
*   **Desktop**: Uses `cpal` (Native).
*   **Web**: Uses Browser WebAudio API (via specialized frontend JS/WASM).
*   **Strategy**: Keep audio *capture* in the frontend layer. Send audio *data* (blobs/streams) to the backend for processing (transcription).
*   If `noema-audio` has shared processing logic (VAD, Transcription), it can remain a shared crate used by both.

### 4. Multiple Users
*   All service methods must require `user_id`.
*   `noema-desktop` will likely use a constant `user_id` (e.g., "local-user") or one derived from OS user.
*   `noema-web` will derive `user_id` from the session token.

### 5. Different Storage (Postgres vs Sqlite)
*   **Desktop**: Wraps `SqliteStore` (from `noema-core`) to implement `StorageProvider`.
*   **Web**: Create a new `PostgresStore` (in a new crate or feature-gated) implementing `StorageProvider`.

## 5. Migration Steps

### Phase 0: Refactor `noema-core` Storage
1.  **Define `BlobStore` Trait**:
    *   Add `BlobStore` trait to `noema-core/src/storage/traits.rs`.
    *   Make it `async_trait` to support future cloud implementations (S3).
    *   Methods: `put`, `get`, `exists`, `delete`, `list_all`.
2.  **Refactor Existing Implementation**:
    *   Rename `noema_core::storage::BlobStore` to `FsBlobStore`.
    *   Implement the `BlobStore` trait for `FsBlobStore` (using `tokio::fs`).
3.  **Update Consumers**:
    *   Update `noema-desktop` to instantiate `FsBlobStore` but use it via the trait where possible.

### Phase 1: Setup & Definition
1.  Initialize `noema-backend`.
2.  Define `StorageProvider` and `BlobStorageProvider` traits in `noema-backend`.
3.  Create `SqliteStorageAdapter` in `noema-backend` (or `noema-desktop` if we want to keep backend pure) that wraps `noema_core::SqliteStore`.

### Phase 2: Logic Extraction
1.  **Move Types**: Move shared types (`DisplayMessage`, `Attachment`, etc.) from `noema-desktop` to `noema-backend/src/types.rs`.
2.  **Extract Chat Logic**: Move `send_message_internal` and the event loop logic from `noema-desktop` to `ChatService` in `noema-backend`.
    *   *Challenge*: The current event loop relies on Tauri's `AppHandle` to emit events.
    *   *Solution*: `ChatService` should return a Rust `Stream` of events. The Tauri command will consume this stream and emit Tauri events.
3.  **Extract Conversation Logic**: Move list/create/delete logic to `ConversationService`.

### Phase 3: Refactor Desktop
1.  Update `AppState` in `noema-desktop` to hold `Arc<ChatService>`, `Arc<ConversationService>` instead of raw `SqliteStore`.
2.  Rewrite Tauri commands to simply call Service methods.
3.  Map the Service `Stream` outputs to Tauri Events.

### Phase 4: Verification
1.  Verify Desktop app functionality (Chat, History, Settings).
2.  Ensure no regressions in MCP or Local RAG features.

## 6. Example: Chat Service Interface

```rust
// noema-backend/src/services/chat.rs

pub struct ChatService {
    storage: Arc<dyn StorageProvider>,
    mcp_registry: Arc<McpRegistry>,
}

impl ChatService {
    pub async fn send_message(
        &self, 
        user_id: &str, 
        conversation_id: &str, 
        payload: ChatPayload
    ) -> impl Stream<Item = EngineEvent> {
        // ... logic previously in send_message_internal ...
    }
}
```

## 7. Folder Structure for `noema-backend`

```
noema-backend/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── error.rs
    ├── types.rs          # Shared DTOs
    ├── ports/            # Traits
    │   ├── storage.rs
    │   └── blob.rs
    └── services/         # Business Logic
        ├── chat.rs
        ├── conversation.rs
        └── user.rs
```
